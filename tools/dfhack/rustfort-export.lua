-- rustfort-export.lua - READ-ONLY exporter of the loaded DF fort into the Rustfort export format
-- (docs/EXPORT_FORMAT.md, format_version 1).  DF 53.16 + DFHack 53.16-r2.
--
--   dfhack-run lua -f <abs path>/rustfort-export.lua <out_dir> [options]
--     --budget MS      Lua time per game frame (default 25)
--     --expect NAME    native fort name that must be loaded (default Tileshtulon); --any-fort skips the check
--     --force          ignore a stale export.lock
--     --skip-map       only entities (units/items/buildings/plants) - for quick re-tests
--     --zmin N --zmax N  only export these z-levels of the map (testing; files of other z stay untouched)
--     --status         print progress.json of <out_dir> and exit
--     --cancel         ask a running export to stop (creates <out_dir>/cancel)
--
-- Safety contract (Gordon's live game, possibly with his own agents playing):
--   * never pauses/unpauses, never writes to DF memory or game state, enables no plugin, installs no hook.
--   * all work runs in a coroutine resumed by one-shot dfhack.timeout(1,'frames') callbacks, each slice
--     limited to --budget ms (BUG-415: a whole-map scan in one go froze DF for 9-28 s).
--   * raw block arrays are copied with dfhack.internal.memmove FROM DF memory INTO buffers this script
--     allocated itself (df.new) - DF memory is only read.
--   * the call returns immediately; manifest.json is written LAST and is the done-marker.
--     progress.json is rewritten about once per second; error.json is written if the export aborts.
--
-- Files: manifest.json tiletypes.json materials.json map/zNNN.bin units.jsonl items.jsonl buildings.jsonl
--        plants.jsonl creatures.json + (round 2) work_orders.jsonl plant_raws.json burrows.json squads.json
--        + (round 3) creature_raws.json weapon_raws.json armor_raws.json

local VERSION = 1
-- round 2 (2026-10-10a): work orders, workshop jobs, crops, plant raws, mods, gaits
-- round 3 (2026-10-10b): needs/emotions/values/relationships/wounds/blood, creature/weapon/armor raws,
--                        material mechanics, squads with orders/uniforms/schedule, burrow tile lists,
--                        corpse creature tokens, heavy aquifer tile flag
local SCRIPT_REV = '2026-10-10b'

-- ------------------------------------------------------------------------------------------- args
local args = { ... }
local opt = { budget = 25, expect = 'Tileshtulon', force = false, skip_map = false }
do
  local i = 1
  while i <= #args do
    local a = args[i]
    if a == '--budget' then opt.budget = tonumber(args[i + 1]) or 25; i = i + 1
    elseif a == '--expect' then opt.expect = args[i + 1]; i = i + 1
    elseif a == '--any-fort' then opt.expect = nil
    elseif a == '--force' then opt.force = true
    elseif a == '--skip-map' then opt.skip_map = true
    elseif a == '--zmin' then opt.zmin = tonumber(args[i + 1]); i = i + 1
    elseif a == '--zmax' then opt.zmax = tonumber(args[i + 1]); i = i + 1
    elseif a == '--status' then opt.status = true
    elseif a == '--cancel' then opt.cancel = true
    elseif not opt.out then opt.out = a
    end
    i = i + 1
  end
end
local OUT = (opt.out or 'C:/Users/admin/claude gordons projects/rustfort/export/tileshtulon'):gsub('\\', '/'):gsub('/+$', '')

local function file_exists(p)
  local f = io.open(p, 'rb')
  if f then f:close() return true end
  return false
end
local function read_file(p)
  local f = io.open(p, 'rb')
  if not f then return nil end
  local s = f:read('a')
  f:close()
  return s
end
local function write_file(p, s)
  local f, e = io.open(p, 'wb')
  if not f then error('cannot write ' .. p .. ': ' .. tostring(e)) end
  f:write(s)
  f:close()
end

if opt.status then
  print(read_file(OUT .. '/progress.json') or '{"phase":"none"}')
  return
end
if opt.cancel then
  write_file(OUT .. '/cancel', 'cancel\n')
  print('{"ok":true,"cancel_requested":true}')
  return
end

-- ------------------------------------------------------------------------------------------- json
local ESC = { ['"'] = '\\"', ['\\'] = '\\\\', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
local function esc_ctl(c) return ESC[c] or string.format('\\u%04x', c:byte()) end
-- DF strings are CP437 -> UTF-8 (only runs of bytes >= 0x80 need converting)
local function utf(s)
  if s == nil then return '' end
  s = tostring(s)
  if s:find('[\128-\255]') then s = s:gsub('[\128-\255]+', dfhack.df2utf) end
  return s
end
local function js(s)
  local u = utf(s):gsub('[%c"\\]', esc_ctl)
  return '"' .. u .. '"'
end
-- our own ASCII strings (no CP437 conversion)
local function ja(s)
  local u = tostring(s or ''):gsub('[%c"\\]', esc_ctl)
  return '"' .. u .. '"'
end
local floor = math.floor
local function ji(v)
  v = tonumber(v)
  if not v or v ~= v or v == math.huge or v == -math.huge then return '0' end
  return string.format('%d', floor(v))
end
local function jf(v)
  v = tonumber(v)
  if not v or v ~= v or v == math.huge or v == -math.huge then return '0' end
  return (string.format('%.4f', v):gsub(',', '.'))
end
local function jb(v) return v and 'true' or 'false' end
local function jarr(list) return '[' .. table.concat(list, ',') .. ']' end
-- object from an ordered list {k1, encoded1, k2, encoded2, ...}
local function jobj(kv)
  local o = {}
  for i = 1, #kv, 2 do o[#o + 1] = '"' .. kv[i] .. '":' .. kv[i + 1] end
  return '{' .. table.concat(o, ',') .. '}'
end
local function jpos(x, y, z) return '{"x":' .. ji(x) .. ',"y":' .. ji(y) .. ',"z":' .. ji(z) .. '}' end

-- ------------------------------------------------------------------------------------------- helpers
local function now() return dfhack.getTickCount() end
local function try(f, ...)
  local ok, a, b, c = pcall(f, ...)
  if ok then return a, b, c end
  return nil
end
local raws = df.global.world.raws
local notes = {}
local function note(s) notes[#notes + 1] = s end
local warn_count = {}
local function warn(kind, msg)
  local n = (warn_count[kind] or 0) + 1
  warn_count[kind] = n
  if n <= 3 then note(kind .. ': ' .. tostring(msg)) end
end

-- ------------------------------------------------------------------------------------------- checks
if not dfhack.isMapLoaded() or df.global.gamemode ~= df.game_mode.DWARF then
  print('{"ok":false,"error":"no fortress loaded"}')
  return
end
local site = df.global.world.world_data.active_site[0]
local FORT_NATIVE = dfhack.translation.translateName(site.name, false)
local FORT_EN = dfhack.translation.translateName(site.name, true)
local SITE_ID = site.id
if opt.expect and FORT_NATIVE ~= opt.expect then
  print(('{"ok":false,"error":"loaded fort is %s (%s), expected %s - not exporting"}'):format(
    utf(FORT_NATIVE), utf(FORT_EN), utf(opt.expect)))
  return
end

dfhack.filesystem.mkdir_recursive(OUT)
dfhack.filesystem.mkdir_recursive(OUT .. '/map')
local LOCK = OUT .. '/export.lock'
do
  local l = read_file(LOCK)
  if l and not opt.force then
    local t = tonumber(l:match('%d+')) or 0
    if os.time() - t < 900 then
      print('{"ok":false,"error":"export.lock present (another export running?) - use --force if stale"}')
      return
    end
  end
end
write_file(LOCK, tostring(os.time()) .. '\n')
os.remove(OUT .. '/cancel')
os.remove(OUT .. '/error.json')
os.remove(OUT .. '/manifest.json')   -- the done-marker of a previous run

-- ------------------------------------------------------------------------------------------- state
local T0 = now()
local START_YEAR, START_TICK = df.global.cur_year, df.global.cur_year_tick
local deadline = 0
local frames, max_slice, total_lua = 0, 0, 0
local phase_max = {}   -- longest slice per phase (ms), reported in manifest.json max_slice_by_phase
local phase, phase_detail = 'start', ''
local last_progress = 0
local cancelled = false
local last_cancel_check = 0

local function progress(force)
  local t = now()
  if not force and t - last_progress < 1000 then return end
  last_progress = t
  pcall(write_file, OUT .. '/progress.json', jobj({
    'phase', ja(phase), 'detail', ja(phase_detail), 'elapsed_s', jf((t - T0) / 1000), 'frames', ji(frames),
    'max_slice_ms', ji(max_slice), 'year_tick', ji(df.global.cur_year_tick), 'rev', ja(SCRIPT_REV) }) .. '\n')
  if t - last_cancel_check > 1000 then
    last_cancel_check = t
    if file_exists(OUT .. '/cancel') then cancelled = true end
  end
end

-- yield to DF when the slice budget is used up
-- returns true if it yielded (callers must re-fetch DF pointers they hold)
local function checkpoint()
  if now() >= deadline then
    progress(false)
    coroutine.yield()
    if cancelled then error('cancelled by user (cancel file)') end
    return true
  end
  return false
end

-- vectors change between frames: snapshot the ids in one cheap pass, then re-find each object by id
local function id_snapshot(vec)
  local ids = {}
  for i = 0, #vec - 1 do ids[i + 1] = vec[i].id end
  return ids
end

-- ------------------------------------------------------------------------------------------- palette
local pal_key, pal = {}, {}
local function palidx(mt, mi)
  if mt == nil or mt < 0 then return 0xFFFF end
  mi = mi or -1
  local key = mt * 4294967296 + (mi + 1)
  local i = pal_key[key]
  if i then return i end
  if #pal >= 65535 then return 0xFFFF end
  pal[#pal + 1] = { mt, mi }
  i = #pal - 1
  pal_key[key] = i
  return i
end
local find_cache = {}
local function palfind(token)
  local v = find_cache[token]
  if v ~= nil then return v end
  v = 0xFFFF
  local m = try(dfhack.matinfo.find, token)
  if m then v = palidx(m.type, m.index) end
  find_cache[token] = v
  return v
end

local function flag(fl, name)
  local ok, v = pcall(function() return fl[name] end)
  return ok and v and true or false
end
local INORG_TAGS = { 'SOIL_SAND', 'DEEP_SPECIAL', 'AQUIFER', 'SEDIMENTARY', 'IGNEOUS_INTRUSIVE', 'IGNEOUS_EXTRUSIVE',
                     'METAMORPHIC', 'SOIL_OCEAN', 'SEDIMENTARY_OCEAN_SHALLOW', 'SEDIMENTARY_OCEAN_DEEP', 'DIVINE',
                     'WAFERS', 'GENERATED' }
local function material_json(mt, mi)
  local token, name, rgb = mt .. ':' .. mi, '', { 128, 128, 128 }
  local st, gem, metal, ore, soil, wood, glass, tags = false, false, false, false, false, false, false, {}
  local info = try(dfhack.matinfo.decode, mt, mi)
  if info then
    token = try(function() return info:getToken() end) or token
    local m = info.material
    name = try(function() return m.state_name.Solid end) or ''
    pcall(function()
      local col = raws.descriptors.colors[m.state_color.Solid]
      rgb = { floor(col.red * 255 + 0.5), floor(col.green * 255 + 0.5), floor(col.blue * 255 + 0.5) }
    end)
    local fl = m.flags
    st, gem, metal, glass = flag(fl, 'IS_STONE'), flag(fl, 'IS_GEM'), flag(fl, 'IS_METAL'), flag(fl, 'IS_GLASS')
    wood = flag(fl, 'WOOD') or token:match(':WOOD$') ~= nil
    pcall(function()
      for i = 0, #m.reaction_class - 1 do tags[#tags + 1] = ja('RC:' .. m.reaction_class[i].value) end
    end)
    if info.inorganic then
      local ifl = info.inorganic.flags
      ore = flag(ifl, 'METAL_ORE')
      soil = flag(ifl, 'SOIL') or flag(ifl, 'SOIL_ANY')
      for _, t in ipairs(INORG_TAGS) do if flag(ifl, t) then tags[#tags + 1] = ja(t == 'SOIL_SAND' and 'IS_SAND' or t) end end
      if soil and token:find('CLAY') then tags[#tags + 1] = ja('IS_CLAY') end
    end
  end
  -- round 3: mechanical properties (DF material.strength, solid_density, material_value) and heat points
  local mech = { 'density', '0', 'shear_yield', '0', 'shear_fracture', '0', 'shear_strain_at_yield', '0',
                 'impact_yield', '0', 'impact_fracture', '0', 'impact_strain_at_yield', '0', 'max_edge', '0', 'value', '0' }
  pcall(function()
    local m = info.material
    local s = m.strength
    local SH, IM = df.strain_type.SHEAR, df.strain_type.IMPACT
    mech = { 'density', ji(m.solid_density), 'shear_yield', ji(s.yield[SH]), 'shear_fracture', ji(s.fracture[SH]),
             'shear_strain_at_yield', ji(s.strain_at_yield[SH]), 'impact_yield', ji(s.yield[IM]),
             'impact_fracture', ji(s.fracture[IM]), 'impact_strain_at_yield', ji(s.strain_at_yield[IM]),
             'max_edge', ji(s.max_edge), 'value', ji(m.material_value) }
  end)
  -- extras: all six strain types [yield, fracture, strain_at_yield], absorption, liquid density, molar mass, heat
  pcall(function()
    local m = info.material
    local s = m.strength
    local st6 = {}
    for i = df.strain_type._first_item, df.strain_type._last_item do
      local nm = df.strain_type[i]
      if nm then st6[#st6 + 1] = '"' .. nm .. '":' .. jarr({ ji(s.yield[i]), ji(s.fracture[i]), ji(s.strain_at_yield[i]) }) end
    end
    local more = { 'strength', '{' .. table.concat(st6, ',') .. '}', 'absorption', ji(s.absorption),
                   'liquid_density', ji(m.liquid_density), 'molar_mass', ji(m.molar_mass) }
    for i = 1, #more do mech[#mech + 1] = more[i] end
  end)
  pcall(function()
    local h = info.material.heat
    local hj = jobj({ 'spec_heat', ji(h.spec_heat), 'heatdam_point', ji(h.heatdam_point),
      'colddam_point', ji(h.colddam_point), 'ignite_point', ji(h.ignite_point), 'melting_point', ji(h.melting_point),
      'boiling_point', ji(h.boiling_point), 'mat_fixed_temp', ji(h.mat_fixed_temp) })
    mech[#mech + 1] = 'heat'; mech[#mech + 1] = hj
  end)
  local kv = { 'mat_type', ji(mt), 'mat_index', ji(mi), 'token', js(token), 'name', js(name),
               'rgb', jarr({ ji(rgb[1]), ji(rgb[2]), ji(rgb[3]) }),
               'is_stone', jb(st), 'is_gem', jb(gem), 'is_metal', jb(metal), 'is_ore', jb(ore), 'is_soil', jb(soil),
               'is_wood', jb(wood), 'is_glass', jb(glass), 'tags', jarr(tags) }
  for i = 1, #mech do kv[#kv + 1] = mech[i] end
  return jobj(kv)
end

-- ------------------------------------------------------------------------------------------- job / order helpers
-- material token of a (type, index) pair as DF names it: '' = any, 'INORGANIC' = any inorganic (0,-1),
-- 'INORGANIC:IRON', 'COAL', 'PLANT:MUSHROOM_HELMET_PLUMP:DRINK', ...
local mat_tok_cache = {}
local function mat_token(mt, mi)
  if mt == nil or mt < 0 then return '' end
  mi = mi or -1
  local key = mt * 4294967296 + (mi + 1)
  local v = mat_tok_cache[key]
  if v then return v end
  local info = try(dfhack.matinfo.decode, mt, mi)
  v = info and try(function() return info:getToken() end) or (mt .. ':' .. mi)
  mat_tok_cache[key] = v
  return v
end
local function item_type_name(t)
  if t == nil or t < 0 then return '' end
  return df.item_type[t] or tostring(t)
end
local function subtype_token(t, st)
  if t == nil or t < 0 or st == nil or st < 0 then return '' end
  local d = try(dfhack.items.getSubtypeDef, t, st)
  return d and d.id or tostring(st)
end
-- enum value of a struct field by name (works for any enum-typed field)
local function enum_name(obj, field)
  return try(function() return obj:_field(field)._type[obj[field]] end) or tostring(try(function() return obj[field] end))
end
-- names of the set bits of a DF bitfield, JSON-encoded; `upper` = uppercase them
local function set_flags(bf, upper)
  local out = {}
  pcall(function()
    for k, v in pairs(bf) do
      if v == true and type(k) == 'string' and not k:match('^UNUSED') then out[#out + 1] = ja(upper and k:upper() or k) end
    end
  end)
  return out
end
-- product item type of a job: the job's own item_type if set, else the job type's product (df.job_type.attrs.item)
local function product_type(jt, it)
  if it ~= nil and it >= 0 then return it end
  return try(function() return df.job_type.attrs[jt].item end) or -1
end
-- the fields shared by rf_core::BuildingJob and rf_core::WorkOrder, as a jobj key/value list
local function job_core(jt, reaction, it, ist, mt, mi, mc)
  local pt = product_type(jt, it)
  return { 'job', ja(df.job_type[jt] or tostring(jt)), 'reaction', js(reaction or ''), 'item_type', ja(item_type_name(pt)),
           'item_subtype', js(subtype_token(pt, ist)), 'mat', js(mat_token(mt, mi)), 'mat_category', jarr(set_flags(mc, false)) }
end
local function kv_append(kv, more)
  for i = 1, #more do kv[#kv + 1] = more[i] end
  return kv
end

-- ------------------------------------------------------------------------------------------- tiletypes
local TT_MAT, TT_SHAPE = {}, {}
local function tiletypes_json()
  local out = {}
  local TS, TM, TP, TV = df.tiletype_shape, df.tiletype_material, df.tiletype_special, df.tiletype_variant
  for id = df.tiletype._first_item, df.tiletype._last_item do
    local nm = df.tiletype[id]
    if nm and id >= 0 then
      local at = df.tiletype.attrs[id]
      TT_MAT[id] = at.material
      TT_SHAPE[id] = at.shape
      local vname = TV[at.variant] or 'NONE'
      local var = tonumber(tostring(vname):match('VAR_(%d)')) or 0
      local dir = try(function() return at.direction end)
      dir = (type(dir) == 'string') and dir or ''
      if dir:match('^%-*$') and #dir > 0 and not dir:find('[NSEW]') then dir = '' end
      out[#out + 1] = jobj({ 'id', ji(id), 'name', ja(nm), 'shape', ja(TS[at.shape] or 'NONE'),
                             'material', ja(TM[at.material] or 'NONE'), 'special', ja(TP[at.special] or 'NONE'),
                             'variant', ji(var), 'direction', ja(dir) })
    end
  end
  return '[\n' .. table.concat(out, ',\n') .. '\n]\n', #out
end

-- ------------------------------------------------------------------------------------------- plants (pass 1)
local XC, YC, ZC = dfhack.maps.getTileSize()
local tree_cols = {}   -- [bx*4096+by] = { {x,y,z,mat}, ... }
local plant_at = {}    -- [(z*YC+y)*XC+x] = palette index (shrubs, saplings)
local wood_of, basic_of = {}, {}
local function plant_wood(ri)
  local v = wood_of[ri]
  if v then return v end
  local id = try(function() return raws.plants.all[ri].id end) or '?'
  v = palfind('PLANT:' .. id .. ':WOOD')
  if v == 0xFFFF then v = palfind('PLANT:' .. id .. ':STRUCTURAL') end
  wood_of[ri] = v
  return v
end
local function plant_basic(ri)
  local v = basic_of[ri]
  if v then return v end
  local id = try(function() return raws.plants.all[ri].id end) or '?'
  v = palfind('PLANT:' .. id .. ':STRUCTURAL')
  if v == 0xFFFF then v = palidx(419, ri) end
  basic_of[ri] = v
  return v
end

local counts = {}
local function export_plants()
  phase = 'plants'
  local f = assert(io.open(OUT .. '/plants.jsonl', 'wb'))
  local buf, n = {}, 0
  local all = df.global.world.plants.all
  local S = df.tiletype_shape
  local total = #all
  for i = 0, total - 1 do
    if i >= #all then break end   -- the vector may shrink between frames
    local ok, line = pcall(function()
      local p = all[i]
      local ri = p.material
      local raw = raws.plants.all[ri]
      local x, y, z = p.pos.x, p.pos.y, p.pos.z
      local is_tree_raw = flag(raw.flags, 'TREE')
      local has_tree = p.tree_info ~= nil
      local dead = try(function() return p.damage_flags.dead end) or false
      local sapling = false
      if is_tree_raw and not has_tree then
        local tt = dfhack.maps.getTileType(x, y, z)
        sapling = tt ~= nil and TT_SHAPE[tt] == S.SAPLING
      end
      if x >= 0 and y >= 0 and z >= 0 and x < XC and y < YC and z < ZC then
        if has_tree then
          local k = (x // 16) * 4096 + (y // 16)
          local col = tree_cols[k]
          if not col then col = {}; tree_cols[k] = col end
          col[#col + 1] = { x, y, z, plant_wood(ri) }
        else
          plant_at[(z * YC + y) * XC + x] = plant_basic(ri)
        end
      end
      return jobj({ 'pos', jpos(x, y, z), 'token', ja(raw.id), 'tree', jb(has_tree or (is_tree_raw and not sapling)),
                    'shrub', jb(not is_tree_raw), 'sapling', jb(sapling), 'dead', jb(dead),
                    'grow_counter', ji(p.grow_counter) })
    end)
    if ok then
      n = n + 1
      buf[#buf + 1] = line
      if #buf >= 512 then f:write(table.concat(buf, '\n'), '\n'); buf = {} end
    else
      warn('plant', line)
    end
    if i % 64 == 0 then phase_detail = i .. '/' .. #all; checkpoint() end
  end
  if #buf > 0 then f:write(table.concat(buf, '\n'), '\n') end
  f:close()
  counts.plants = n
end

-- ------------------------------------------------------------------------------------------- map
local TM = df.tiletype_material
local C_STONE, C_SOIL, C_MINERAL, C_FEATURE, C_LAVA, C_FROZEN, C_CONS = TM.STONE, TM.SOIL, TM.MINERAL, TM.FEATURE,
  TM.LAVA_STONE, TM.FROZEN_LIQUID, TM.CONSTRUCTION
local C_GL, C_GD, C_GY, C_GX = TM.GRASS_LIGHT, TM.GRASS_DARK, TM.GRASS_DRY, TM.GRASS_DEAD
local C_TREE, C_ROOT, C_MUSH, C_PLANT = TM.TREE, TM.ROOT, TM.MUSHROOM, TM.PLANT
local C_MAGMA, C_POOL, C_BROOK, C_RIVER = TM.MAGMA, TM.POOL, TM.BROOK, TM.RIVER
local getPlantAtTile = dfhack.maps.getPlantAtTile

local geo_cache = {}
local function layer_mat(x, y, z, layer)
  local rx, ry = dfhack.maps.getTileBiomeRgn(x, y, z)
  local rb = dfhack.maps.getRegionBiome(rx, ry)
  local geo = rb.geo_index
  local key = geo * 64 + layer
  local v = geo_cache[key]
  if v then return v end
  v = 0xFFFF
  pcall(function()
    v = palidx(0, df.global.world.world_data.geo_biomes[geo].layers[layer].mat_index)
  end)
  geo_cache[key] = v
  return v
end

local function vein_map(blk)
  local m = {}
  for _, ev in ipairs(blk.block_events) do
    if df.block_square_event_mineralst:is_instance(ev) then
      local mat = palidx(0, ev.inorganic_mat)
      for ly = 0, 15 do
        local bits = ev.tile_bitmask.bits[ly]
        if bits ~= 0 then
          for lx = 0, 15 do if bits & (1 << lx) ~= 0 then m[lx * 16 + ly] = mat end end
        end
      end
    end
  end
  return m
end

local function grass_map(blk)
  local m, best = {}, {}
  for _, ev in ipairs(blk.block_events) do
    if df.block_square_event_grassst:is_instance(ev) then
      local mat = plant_basic(ev.plant_index)
      for lx = 0, 15 do
        local col = ev.amount[lx]
        for ly = 0, 15 do
          local a = col[ly]
          local i = lx * 16 + ly
          if a > 0 and a > (best[i] or 0) then best[i] = a; m[i] = mat end
        end
      end
    end
  end
  return m
end

local ADAMANTINE, SLADE
local function feature_mat(blk, d)
  if (d >> 28) & 1 == 1 then
    local v = try(function()
      local fi = dfhack.maps.getLocalInitFeature(blk.region_pos, blk.local_feature)
      if fi.mat_type >= 0 then return palidx(fi.mat_type, fi.mat_index) end
    end)
    if v then return v end
    ADAMANTINE = ADAMANTINE or palfind('INORGANIC:RAW_ADAMANTINE')
    warn('feature_local_approx', 'local feature material -> RAW_ADAMANTINE')
    return ADAMANTINE
  end
  local v = try(function()
    local fi = dfhack.maps.getGlobalInitFeature(blk.global_feature)
    if fi.mat_type >= 0 then return palidx(fi.mat_type, fi.mat_index) end
  end)
  if v then return v end
  SLADE = SLADE or palfind('INORGANIC:SLADE')
  warn('feature_global_approx', 'global feature material -> SLADE')
  return SLADE
end

-- tree candidates around a block (3x3 block columns)
local function tree_candidates(bx, by)
  local c = {}
  for dx = -1, 1 do for dy = -1, 1 do
    local col = tree_cols[(bx + dx) * 4096 + (by + dy)]
    if col then for _, t in ipairs(col) do c[#c + 1] = t end end
  end end
  return c
end
local function nearest_tree(cands, x, y, z)
  local best, bd = nil, 1e9
  for i = 1, #cands do
    local t = cands[i]
    local dz = z - t[3]
    -- trees grow up from their base (roots go a few levels down)
    if dz >= -4 and dz <= 30 then
      local d = (x - t[1]) ^ 2 + (y - t[2]) ^ 2 + (dz < 0 and dz * dz * 0.25 or 0)
      if d < bd then bd, best = d, t end
    end
  end
  return best and best[4] or nil
end

local unresolved = {}
local stat = { tree_exact = 0, tree_nearest = 0 }
local function export_map()
  phase = 'map'
  local bt = df.new('uint16_t', 256)
  local bd = df.new('uint32_t', 256)
  local bo = df.new('uint32_t', 256)
  local _, a_bt = df.sizeof(bt)
  local _, a_bd = df.sizeof(bd)
  local _, a_bo = df.sizeof(bo)
  local raw_ok = nil
  local cpos = df.coord:new()
  local WATER = palidx(6, -1)
  local OBSIDIAN = palfind('INORGANIC:OBSIDIAN')
  local pack = string.pack
  local VOID_SEG = string.rep(pack('<I2I2I2BB', 0, 0xFFFF, 1, 0, 0), 16)
  local BX, BY = XC // 16, YC // 16
  local z0, z1 = math.max(0, opt.zmin or 0), math.min(ZC - 1, opt.zmax or (ZC - 1))
  local tiles = 0
  local ok, err = pcall(function()
    for z = z0, z1 do
      phase_detail = 'z ' .. z .. '/' .. (ZC - 1)
      local rows = {}
      for y = 0, YC - 1 do rows[y] = {} end
      for by = 0, BY - 1 do
        for bx = 0, BX - 1 do
          local blk = dfhack.maps.getBlock(bx, by, z)
          local x0, y0 = bx * 16, by * 16
          if not blk then
            for ly = 0, 15 do rows[y0 + ly][bx + 1] = VOID_SEG end
          else
            if raw_ok == nil then
              raw_ok = (df.sizeof(blk.tiletype) == 512 and df.sizeof(blk.designation) == 1024
                        and df.sizeof(blk.occupancy) == 1024)
              if not raw_ok then note('block array sizes differ from 53.16 - using slow field access') end
            end
            if raw_ok then
              local _, s1 = df.sizeof(blk.tiletype)
              local _, s2 = df.sizeof(blk.designation)
              local _, s3 = df.sizeof(blk.occupancy)
              dfhack.internal.memmove(a_bt, s1, 512)
              dfhack.internal.memmove(a_bd, s2, 1024)
              dfhack.internal.memmove(a_bo, s3, 1024)
            else
              for lx = 0, 15 do for ly = 0, 15 do
                local i = lx * 16 + ly
                bt[i] = blk.tiletype[lx][ly]
                bd[i] = blk.designation[lx][ly].whole
                bo[i] = blk.occupancy[lx][ly].whole
              end end
            end
            local lcache, veins, grass, trees, feat = {}, nil, nil, nil, nil
            for ly = 0, 15 do
              local recs = {}
              local y = y0 + ly
              for lx = 0, 15 do
                local i = lx * 16 + ly
                local tt, d, o = bt[i], bd[i], bo[i]
                local cls = TT_MAT[tt]
                local mat = 0xFFFF
                if cls == C_STONE or cls == C_SOIL then
                  local key = ((d >> 17) & 15) * 16 + ((d >> 10) & 15)
                  mat = lcache[key]
                  if not mat then
                    mat = try(layer_mat, x0 + lx, y, z, (d >> 10) & 15) or 0xFFFF
                    lcache[key] = mat
                  end
                elseif cls == C_MINERAL then
                  veins = veins or vein_map(blk)
                  mat = veins[i]
                  if not mat then
                    mat = try(layer_mat, x0 + lx, y, z, (d >> 10) & 15) or 0xFFFF
                    unresolved.mineral = (unresolved.mineral or 0) + 1
                  end
                elseif cls == C_CONS then
                  cpos.x, cpos.y, cpos.z = x0 + lx, y, z
                  local con = df.construction.find(cpos)
                  if con then mat = palidx(con.mat_type, con.mat_index)
                  else unresolved.construction = (unresolved.construction or 0) + 1 end
                elseif cls == C_GL or cls == C_GD or cls == C_GY or cls == C_GX then
                  grass = grass or grass_map(blk)
                  mat = grass[i]
                  if not mat then mat = 0xFFFF; unresolved.grass = (unresolved.grass or 0) + 1 end
                elseif cls == C_TREE or cls == C_ROOT or cls == C_MUSH then
                  -- exact: DFHack resolves the tree whose body covers the tile; else nearest trunk
                  local pl = getPlantAtTile and try(getPlantAtTile, x0 + lx, y, z)
                  if pl then mat = plant_wood(pl.material); stat.tree_exact = stat.tree_exact + 1
                  else
                    trees = trees or tree_candidates(bx, by)
                    mat = nearest_tree(trees, x0 + lx, y, z)
                    if mat then stat.tree_nearest = stat.tree_nearest + 1
                    else mat = 0xFFFF; unresolved.tree = (unresolved.tree or 0) + 1 end
                  end
                elseif cls == C_PLANT then
                  mat = plant_at[(z * YC + y) * XC + x0 + lx]
                  if not mat then mat = 0xFFFF; unresolved.plant = (unresolved.plant or 0) + 1 end
                elseif cls == C_FROZEN then
                  mat = WATER
                elseif cls == C_LAVA or cls == C_MAGMA then
                  mat = OBSIDIAN   -- lava stone / semi-molten rock (approximation)
                elseif cls == C_POOL or cls == C_BROOK or cls == C_RIVER then
                  local key = ((d >> 17) & 15) * 16 + ((d >> 10) & 15)
                  mat = lcache[key]
                  if not mat then
                    mat = try(layer_mat, x0 + lx, y, z, (d >> 10) & 15) or 0xFFFF
                    lcache[key] = mat
                  end
                elseif cls == C_FEATURE then
                  feat = feat or feature_mat(blk, d)
                  mat = feat
                end
                -- flags (rf_core::tile_flags)
                local sm = (d >> 7) & 3
                local fl = ((d >> 9) & 1)                -- hidden
                  | (((d >> 16) & 1) << 1)               -- outside
                  | (((d >> 15) & 1) << 2)               -- subterranean
                  | (((d >> 14) & 1) << 3)               -- light
                  | (((d >> 22) & 1) << 4)               -- aquifer (water_table)
                  | (((d >> 3) & 1) << 5)                -- pile
                  | (((d >> 24) & 3) << 6)               -- traffic
                  | ((sm == 1) and (1 << 8) or 0)        -- smooth designation
                  | ((sm == 2) and (1 << 9) or 0)        -- engrave designation
                  | (((o >> 24) & 1) << 10)              -- dig_marked (marker/blueprint)
                  | (((o & 7) ~= 0) and (1 << 11) or 0)  -- building occupancy
                  | (((d >> 30) & 1) << 12)              -- stagnant
                  | (((d >> 31) & 1) << 13)              -- salt
                  | (((o >> 26) & 1) << 14)              -- heavy aquifer (o.heavy_aquifer, round 3)
                local liq = (d & 7) | (((d >> 21) & 1) << 3)
                recs[lx + 1] = pack('<I2I2I2BB', tt & 0xFFFF, mat, fl, liq, (d >> 4) & 7)
              end
              rows[y][bx + 1] = table.concat(recs)
              -- blocks full of tree tiles are slow (getPlantAtTile): allow a yield per row; the
              -- tile data is already copied, only the block pointer must be fetched again
              if ly < 15 and checkpoint() then
                blk = dfhack.maps.getBlock(bx, by, z)
                if not blk then
                  for ry = ly + 1, 15 do rows[y0 + ry][bx + 1] = VOID_SEG end
                  break
                end
              end
            end
            tiles = tiles + 256
          end
          checkpoint()
        end
      end
      local f = assert(io.open(('%s/map/z%03d.bin'):format(OUT, z), 'wb'))
      for y = 0, YC - 1 do f:write(table.concat(rows[y])) end
      f:close()
      checkpoint()
    end
  end)
  df.delete(bt); df.delete(bd); df.delete(bo); df.delete(cpos)
  if not ok then error(err, 0) end
  counts.tiles = tiles
  counts.z_exported = z1 - z0 + 1
end

-- ------------------------------------------------------------------------------------------- units
local function enum_list(enum_t, get)
  local out = {}
  for i = enum_t._first_item, enum_t._last_item do
    local nm = enum_t[i]
    if nm then
      local v = try(get, i)
      if v ~= nil then out[#out + 1] = jarr({ ja(nm), ji(v) }) end
    end
  end
  return out
end
local LABOR_FIRST, LABOR_LAST = df.unit_labor._first_item, df.unit_labor._last_item
local race_seen = {}
-- walk_speed: full_speed of the caste's default gait (caste.body_info.gait_info[gait][default_gait_index[gait]]);
-- gait = WALK, or FLY for FLIER castes that have a fly gait. DF units: 900 = dwarf "Walk".
local GAIT_WALK = (df.gait_type and df.gait_type.WALK) or 0
local GAIT_FLY = (df.gait_type and df.gait_type.FLY) or 1
local gait_cache = {}
local gait_stat = { raw = 0, fallback = 0, fly = 0 }
local function caste_walk_speed(race, ci, caste)
  local key = race * 1024 + ci
  local v = gait_cache[key]
  if v == nil then
    v = false
    pcall(function()
      local bi = caste.body_info
      local gt = GAIT_WALK
      if flag(caste.flags, 'FLIER') and bi.default_gait_index[GAIT_FLY] >= 0 then gt = GAIT_FLY end
      local idx = bi.default_gait_index[gt]
      if idx >= 0 then
        local s = bi.gait_info[gt][idx].full_speed
        if s and s > 0 then v = s; if gt == GAIT_FLY then gait_stat.fly = gait_stat.fly + 1 end end
      end
    end)
    gait_cache[key] = v
  end
  return v
end
local function walk_speed(u, caste)
  local v = caste_walk_speed(u.race, u.caste, caste)
  if v then gait_stat.raw = gait_stat.raw + 1 return v end
  gait_stat.fallback = gait_stat.fallback + 1
  return 900
end
local function q(f, u) local ok, r = pcall(f, u) return ok and r or false end

-- ---- round 3: psychology, relationships, wounds
local r3 = { needs = 0, emotions = 0, values = 0, relationships = 0, wounds = 0, invaders = 0 }
local FORT_CIV = try(function() return df.global.plotinfo.civ_id end) or -1
local function needs_json(p)
  local out = {}
  for _, n in ipairs(p.needs) do
    out[#out + 1] = jobj({ 'id', ja(df.need_type[n.id] or tostring(n.id)), 'deity_id', ji(n.deity_id),
                           'focus_level', ji(n.focus_level), 'need_level', ji(n.need_level) })
  end
  return out
end
local function emotions_json(p)
  local out = {}
  for _, e in ipairs(p.emotions) do
    out[#out + 1] = jobj({ 'emotion', ja(df.emotion_type[e.type] or tostring(e.type)),
                           'thought', ja(df.unit_thought_type[e.thought] or tostring(e.thought)),
                           'subthought', ji(e.subthought), 'strength', ji(e.strength), 'severity', ji(e.severity),
                           'year', ji(e.year), 'year_tick', ji(e.year_tick), 'flags', jarr(set_flags(e.flags, true)),
                           -- extras
                           'relative_strength', ji(e.relative_strength) })
  end
  return out
end
local function values_json(p)
  local out = {}
  for _, v in ipairs(p.values) do
    out[#out + 1] = jobj({ 'id', ja(df.value_type[v.type] or tostring(v.type)), 'strength', ji(v.strength) })
  end
  return out
end
-- histfig link type -> rf_core::Relationship kind
local LINK_KIND = { MOTHER = 'Mother', FATHER = 'Father', SPOUSE = 'Spouse', CHILD = 'Child', DEITY = 'Deity',
                    LOVER = 'Lover', PRISONER = 'Prisoner', IMPRISONER = 'Imprisoner', MASTER = 'Master',
                    APPRENTICE = 'Apprentice', COMPANION = 'Companion', FORMER_MASTER = 'FormerMaster',
                    FORMER_APPRENTICE = 'FormerApprentice', PET_OWNER = 'PetOwner', FORMER_SPOUSE = 'FormerSpouse',
                    DECEASED_SPOUSE = 'DeceasedSpouse' }
-- unit id of a historical figure if that unit is alive and on the map, else -1; also the raw unit id
local function hf_unit(hfid)
  local h = df.historical_figure.find(hfid)
  if not h then return -1, -1, false end
  local dead = h.died_year ~= -1
  local uid = h.unit_id or -1
  if uid < 0 then return -1, -1, dead end
  local tu = df.unit.find(uid)
  if tu and not q(dfhack.units.isDead, tu) and q(dfhack.units.isActive, tu) then return uid, uid, dead end
  return -1, uid, dead
end
-- DF v50 relationship level from the visual relationship (wiki "Relationship"): friend at love >= 50,
-- close friend >= 75, kindred spirit 100; long-term acquaintance after 30 meetings. Grudge = love < 0 (estimate).
local function visual_kind(core, meets)
  local love = core.love
  if love < 0 then return 'Grudge' end
  if love >= 100 then return 'KindredSpirit' end
  if love >= 75 then return 'CloseFriend' end
  if love >= 50 then return 'Friend' end
  if meets >= 30 then return 'LongTermAcquaintance' end
  return 'PassingAcquaintance'
end
local function relationships_json(u)
  local out = {}
  local h = u.hist_figure_id >= 0 and df.historical_figure.find(u.hist_figure_id) or nil
  local seen = {}
  local function add(kind, hfid, extra)
    local key = kind .. ':' .. hfid
    if seen[key] then return end
    seen[key] = true
    local uid, uid_any, dead = hf_unit(hfid)
    local kv = { 'kind', ja(kind), 'hist_fig_id', ji(hfid), 'unit_id', ji(uid), 'unit_id_any', ji(uid_any), 'dead', jb(dead) }
    if extra then for i = 1, #extra do kv[#kv + 1] = extra[i] end end
    out[#out + 1] = jobj(kv)
  end
  if h then
    local parents = {}
    for _, l in ipairs(h.histfig_links) do
      local lt = df.histfig_hf_link_type[l:getType()] or ''
      local kind = LINK_KIND[lt] or lt
      add(kind, l.target_hf, { 'source', ja('family'), 'strength', ji(l.link_strength) })
      if lt == 'MOTHER' or lt == 'FATHER' then parents[#parents + 1] = l.target_hf end
    end
    -- siblings: other children of the parents
    for _, ph in ipairs(parents) do
      local p = df.historical_figure.find(ph)
      if p then
        for _, l in ipairs(p.histfig_links) do
          if df.histfig_hf_link_type[l:getType()] == 'CHILD' and l.target_hf ~= h.id then
            add('Sibling', l.target_hf, { 'source', ja('family'), 'strength', ji(l.link_strength) })
          end
        end
      end
    end
    pcall(function()
      for _, r in ipairs(h.info.relationships.hf_visual) do
        local c = r.core
        add(visual_kind(c, r.meet_count), r.histfig_id, { 'source', ja('visual'), 'love', ji(c.love), 'trust', ji(c.trust),
          'respect', ji(c.respect), 'loyalty', ji(c.loyalty), 'fear', ji(c.fear), 'meet_count', ji(r.meet_count),
          'first_year', ji(r.first_year), 'last_meet_year', ji(r.last_meet_year),
          'last_meet_tick', ji(r.last_meet_season_tick) })
      end
    end)
  end
  -- pets: the owner unit (animals have no histfig links)
  pcall(function()
    local owner = u.relationship_ids.PetOwner
    if owner >= 0 then
      local ou = df.unit.find(owner)
      out[#out + 1] = jobj({ 'kind', ja('PetOwner'), 'hist_fig_id', ji(ou and ou.hist_figure_id or -1), 'unit_id', ji(owner),
                             'unit_id_any', ji(owner), 'dead', jb(ou and q(dfhack.units.isDead, ou) or false), 'source', ja('unit') })
    end
  end)
  return out
end
-- unit_wound + its layer entries, flattened per (wound, part, layer) as rf_core::Wound
local function wounds_json(u)
  local out = {}
  local parts = u.body.body_plan.body_parts
  for _, w in ipairs(u.body.wounds) do
    local wflags = set_flags(w.flags, true)
    if w.flags.severed_part then wflags[#wflags + 1] = ja('SEVERED') end
    if w.flags.infection then wflags[#wflags + 1] = ja('INFECTED') end
    local function entry(p)
      local fl, seen = {}, {}
      local function addf(s) if not seen[s] then seen[s] = true; fl[#fl + 1] = s end end
      for _, s in ipairs(wflags) do addf(s) end
      local bp_tok, layer, ext = '', '', {}
      if p then
        for _, s in ipairs(set_flags(p.flags1, true)) do addf(s) end
        for _, s in ipairs(set_flags(p.flags2, true)) do addf(s) end
        if p.bleeding > 0 then addf(ja('BLEEDING')) end
        pcall(function()
          local bp = parts[p.body_part_id]
          bp_tok = bp.token
          layer = p.layer_idx >= 0 and bp.layers[p.layer_idx].layer_name or ''
        end)
        ext = { 'part_id', ji(p.body_part_id), 'layer_idx', ji(p.layer_idx), 'global_layer_idx', ji(p.global_layer_idx),
                'contact_area', ji(p.contact_area), 'surface_perc', ji(p.surface_perc), 'strain', ji(p.strain),
                'cur_penetration_perc', ji(p.cur_penetration_perc), 'max_penetration_perc', ji(p.max_penetration_perc),
                'swelling', ji(p.swelling), 'impaired', ji(p.impaired), 'numbness', ji(p.numbness),
                'paralysis', ji(p.paralysis), 'nausea', ji(p.nausea), 'dizziness', ji(p.dizziness) }
      end
      local kv = { 'body_part', ja(bp_tok), 'layer', ja(layer), 'flags', jarr(fl),
                   -- severity: share of the layer's surface hit, 0..10000 (estimate; raw values in the extras)
                   'severity', ji(p and math.max(0, math.min(100, p.surface_perc)) * 100 or 0),
                   'pain', ji(p and p.pain or 0), 'bleeding', ji(p and p.bleeding or 0), 'age', ji(w.age),
                   -- extras
                   'wound_id', ji(w.id), 'wound_pain', ji(w.pain), 'attacker_unit_id', ji(w.attacker_unit_id),
                   'attacker_hist_figure_id', ji(w.attacker_hist_figure_id), 'syndrome_id', ji(w.syndrome_id),
                   'fever', ji(w.fever) }
      for i = 1, #ext do kv[#kv + 1] = ext[i] end
      out[#out + 1] = jobj(kv)
    end
    if #w.parts == 0 then entry(nil) else for _, p in ipairs(w.parts) do entry(p) end end
  end
  return out
end
local function unit_json(u)
  local U = dfhack.units
  local cr = raws.creatures.all[u.race]
  local caste = cr.caste[u.caste]
  race_seen[u.race] = true
  local vis = try(U.getVisibleName, u)
  local name = vis and try(dfhack.translation.translateName, vis, true) or ''
  local native = vis and try(dfhack.translation.translateName, vis, false) or ''
  if name == '' then name = try(U.getReadableName, u) or cr.name[0] end
  local soul = u.status.current_soul
  local skills, ment, facets = {}, {}, {}
  if soul then
    for _, s in ipairs(soul.skills) do
      skills[#skills + 1] = jobj({ 'id', ja(df.job_skill[s.id] or tostring(s.id)), 'rating', ji(s.rating),
                                   'xp', ji(s.experience) })
    end
    ment = enum_list(df.mental_attribute_type, function(i) return soul.mental_attrs[i].value end)
    facets = enum_list(df.personality_facet_type, function(i) return soul.personality.traits[i] end)
  end
  local phys = enum_list(df.physical_attribute_type, function(i) return u.body.physical_attrs[i].value end)
  local labors = {}
  pcall(function()
    local lab = u.status.labors
    for i = LABOR_FIRST, LABOR_LAST do
      if i >= 0 and lab[i] then labors[#labors + 1] = ja(df.unit_labor[i]) end
    end
  end)
  local inv = {}
  for _, iv in ipairs(u.inventory) do inv[#inv + 1] = ji(iv.item.id) end
  local mood = ''
  pcall(function() if u.mood >= 0 then mood = df.mood_type[u.mood] or '' end end)
  local stress = try(function() return soul.personality.stress end) or 0
  local hostile = q(U.isInvader, u) or q(U.isUndead, u) or (q(U.isDanger, u) and not q(U.isTame, u))
  local flags = jobj({
    'citizen', jb(q(U.isCitizen, u)), 'resident', jb(q(U.isResident, u)), 'animal', jb(q(U.isAnimal, u)),
    'tame', jb(q(U.isTame, u)), 'dead', jb(q(U.isDead, u)),
    'hostile', jb(hostile),
    'visitor', jb(q(U.isVisitor, u)), 'merchant', jb(q(U.isMerchant, u)), 'caged', jb(u.flags1.caged),
    'forest', jb(q(U.isForest, u)), 'inactive', jb(not q(U.isActive, u)) })
  -- round 3: psychology, relationships, wounds, blood, squad position, invader civ
  local needs, emotions, values, rels, wounds = {}, {}, {}, {}, {}
  if soul then
    local p = soul.personality
    local ok, r = pcall(needs_json, p); if ok then needs = r else warn('unit_needs', r) end
    ok, r = pcall(emotions_json, p); if ok then emotions = r else warn('unit_emotions', r) end
    ok, r = pcall(values_json, p); if ok then values = r else warn('unit_values', r) end
  end
  do
    local ok, r = pcall(relationships_json, u); if ok then rels = r else warn('unit_relationships', r) end
    ok, r = pcall(wounds_json, u); if ok then wounds = r else warn('unit_wounds', r) end
  end
  r3.needs, r3.emotions, r3.values = r3.needs + #needs, r3.emotions + #emotions, r3.values + #values
  r3.relationships, r3.wounds = r3.relationships + #rels, r3.wounds + #wounds
  -- the entity a hostile unit belongs to (invaders, raiders); -1 for wildlife, undead without civ and our own
  local invader_civ = -1
  if hostile and u.civ_id >= 0 and u.civ_id ~= FORT_CIV then invader_civ = u.civ_id; r3.invaders = r3.invaders + 1 end
  local size_raw = try(function() return u.body.size_info.size_cur end) or 0
  local focus = try(function() return soul.personality.current_focus end) or 0
  local undistracted = try(function() return soul.personality.undistracted_focus end) or 0
  local stress_cat = try(U.getStressCategory, u)
  local r3kv = {
    'needs', jarr(needs), 'emotions', jarr(emotions), 'values', jarr(values), 'relationships', jarr(rels),
    'wounds', jarr(wounds), 'blood', ji(u.body.blood_count), 'blood_max', ji(u.body.blood_max),
    'squad_position', ji(u.military.squad_position), 'invader_civ', ji(invader_civ),
    -- extras
    'body_size_raw', ji(size_raw), 'current_focus', ji(focus), 'undistracted_focus', ji(undistracted),
    'stress_category', ji(stress_cat or -1),
    'longterm_stress', ji(try(function() return soul.personality.longterm_stress end) or 0),
    'population_id', ji(u.population_id), 'invasion_id', ji(try(function() return u.invasion_id end) or -1),
    'last_attacker', ji(try(function() return u.relationship_ids.LastAttacker end) or -1),
  }
  local kv = {
    'id', ji(u.id), 'race', ja(cr.creature_id), 'caste', ja(caste.caste_id), 'name', js(name), 'name_native', js(native),
    'profession', ja(df.profession[u.profession] or ''), 'custom_profession', js(u.custom_profession),
    'pos', jpos(u.pos.x, u.pos.y, u.pos.z), 'flags', flags, 'civ_id', ji(u.civ_id), 'hist_fig_id', ji(u.hist_figure_id),
    'sex', ji(u.sex), 'age', jf(try(U.getAge, u, true) or 0),
    'hunger_timer', ji(u.counters2.hunger_timer), 'thirst_timer', ji(u.counters2.thirst_timer),
    'sleepiness_timer', ji(u.counters2.sleepiness_timer), 'stress', ji(stress),
    'skills', jarr(skills), 'labors', jarr(labors), 'phys_attrs', jarr(phys), 'ment_attrs', jarr(ment),
    'facets', jarr(facets), 'inventory', jarr(inv), 'squad_id', ji(u.military.squad_id), 'mood', ja(mood),
    -- DF keeps volumes in units of 10 cm³ (raw BODY_SIZE / 10); body_size is cm³ since rev 2026-10-10b
    'body_size', ji(size_raw * 10), 'walk_speed', ji(walk_speed(u, caste)),
    'profession_name', js(try(U.getProfessionName, u) or ''), 'readable_name', js(try(U.getReadableName, u) or '') }
  for i = 1, #r3kv do kv[#kv + 1] = r3kv[i] end
  return jobj(kv)
end

local function export_units()
  phase = 'units'
  local f = assert(io.open(OUT .. '/units.jsonl', 'wb'))
  local ids = id_snapshot(df.global.world.units.all)
  local n, cit, dead, gone, vanished = 0, 0, 0, 0, 0
  local U = dfhack.units
  for i = 1, #ids do
    local u = df.unit.find(ids[i])
    if not u then vanished = vanished + 1
    else
      local is_dead = q(U.isDead, u)
      local is_cit = q(U.isCitizen, u)
      if is_dead then dead = dead + 1
      elseif not (q(U.isActive, u) or is_cit or q(U.isResident, u)) then gone = gone + 1
      else
        local ok, line = pcall(unit_json, u)
        if ok then
          f:write(line, '\n')
          n = n + 1
          if is_cit then cit = cit + 1 end
        else
          warn('unit', line)
        end
      end
    end
    -- round 3 units are heavier (emotions, relationships, wounds): check the budget after every unit
    phase_detail = i .. '/' .. #ids
    checkpoint()
  end
  f:close()
  counts.units, counts.citizens = n, cit
  note(('units: skipped %d dead and %d inactive non-citizen units of world.units.all (%d); %d vanished during export')
       :format(dead, gone, #ids, vanished))
  -- creatures.json: races present in the export
  local cs = {}
  for ri in pairs(race_seen) do
    pcall(function()
      local cr = raws.creatures.all[ri]
      local castes = {}
      for _, c in ipairs(cr.caste) do castes[#castes + 1] = jobj({ 'token', ja(c.caste_id), 'name', js(c.caste_name[0]) }) end
      cs[#cs + 1] = jobj({ 'index', ji(ri), 'token', ja(cr.creature_id), 'name', js(cr.name[0]), 'name_plural', js(cr.name[1]),
                           'castes', jarr(castes) })
    end)
  end
  write_file(OUT .. '/creatures.json', '[\n' .. table.concat(cs, ',\n') .. '\n]\n')
end

-- ------------------------------------------------------------------------------------------- items
local function item_pos(it)
  local a, b, c = dfhack.items.getPosition(it)
  if a == nil then return nil end
  if type(a) == 'number' then return a, b, c end
  return a.x, a.y, a.z
end
local IT_ARTIFACT_REF = df.general_ref_type.IS_ARTIFACT
local CORPSE_TYPES = { [df.item_type.CORPSE] = true, [df.item_type.CORPSEPIECE] = true, [df.item_type.REMAINS] = true }
local corpse_races = {}   -- races of corpses/remains on the map (their raws are exported too: butchery)
local function item_json(it, x, y, z)
  local t = it:getType()
  local subtype = ''
  local extra = {}
  if it:getSubtype() >= 0 then subtype = try(function() return it.subtype.id end) or '' end
  if CORPSE_TYPES[t] then
    -- round 3: corpses, body parts and vermin remains carry the creature token (caste as an extra)
    pcall(function()
      local cr = raws.creatures.all[it.race]
      corpse_races[it.race] = true
      subtype = cr.creature_id
      extra = { 'caste', ja(it.caste >= 0 and cr.caste[it.caste].caste_id or '') }
    end)
  end
  local fl = it.flags
  local quality = try(function() return it:getQuality() end) or 0
  if fl.artifact then quality = 6 end
  local cont = try(dfhack.items.getContainer, it)
  local holder = try(dfhack.items.getHolderUnit, it)
  local bld = try(dfhack.items.getHolderBuilding, it)
  local name = ''
  if fl.artifact then
    pcall(function()
      local ref = dfhack.items.getGeneralRef(it, IT_ARTIFACT_REF)
      local ar = df.artifact_record.find(ref.artifact_id)
      name = dfhack.translation.translateName(ar.name, true)
    end)
  end
  local desc = try(dfhack.items.getReadableDescription, it) or try(dfhack.items.getDescription, it, 0) or ''
  local flags = jobj({ 'on_ground', jb(fl.on_ground), 'in_inventory', jb(fl.in_inventory), 'in_building', jb(fl.in_building),
                       'in_container', jb(cont ~= nil), 'in_job', jb(fl.in_job), 'forbid', jb(fl.forbid), 'dump', jb(fl.dump),
                       'owned', jb(fl.owned), 'rotten', jb(fl.rotten), 'artifact', jb(fl.artifact),
                       'construction', jb(fl.construction), 'hidden', jb(fl.hidden) })
  local wear = try(function() return it:getWear() end) or 0
  local kv = { 'id', ji(it.id), 'kind', ja(df.item_type[t] or tostring(t)), 'subtype', ja(subtype),
               'mat', ji(palidx(it:getMaterial(), it:getMaterialIndex())), 'pos', jpos(x, y, z),
               'stack', ji(try(function() return it:getStackSize() end) or 1), 'quality', ji(math.max(0, math.min(255, quality))),
               'wear', ji(math.max(0, math.min(255, wear))), 'flags', flags,
               'container', ji(cont and cont.id or -1), 'holder', ji(holder and holder.id or -1),
               'building', ji(bld and bld.id or -1), 'name', js(name), 'desc', js(desc),
               'value', ji(try(dfhack.items.getValue, it) or 0) }
  for i = 1, #extra do kv[#kv + 1] = extra[i] end
  return jobj(kv)
end

local function export_items()
  phase = 'items'
  local f = assert(io.open(OUT .. '/items.jsonl', 'wb'))
  local ids = id_snapshot(df.global.world.items.all)
  local n, off, removed = 0, 0, 0
  local buf = {}
  for i = 1, #ids do
    local it = df.item.find(ids[i])
    if not it or it.flags.removed or it.flags.garbage_collect then removed = removed + 1
    else
      local x, y, z = item_pos(it)
      if not x or x < 0 then off = off + 1
      else
        local ok, line = pcall(item_json, it, x, y, z)
        if ok then
          n = n + 1
          buf[#buf + 1] = line
          if #buf >= 256 then f:write(table.concat(buf, '\n'), '\n'); buf = {} end
        else
          warn('item', line)
        end
      end
    end
    if i % 16 == 0 then phase_detail = i .. '/' .. #ids; checkpoint() end
  end
  if #buf > 0 then f:write(table.concat(buf, '\n'), '\n') end
  f:close()
  counts.items = n
  note(('items: skipped %d off-map and %d removed/vanished items of world.items.all (%d)'):format(off, removed, #ids))
end

-- ------------------------------------------------------------------------------------------- buildings
local BT = df.building_type
local function subtype_name(bld, t)
  return try(function()
    if t == BT.Workshop or t == BT.Furnace then
      local s = (t == BT.Workshop and df.workshop_type or df.furnace_type)[bld:getSubtype()]
      if s == 'Custom' then
        local def = df.building_def.find(bld:getCustomType())
        return def and def.code or 'Custom'
      end
      return s
    elseif t == BT.Trap then return df.trap_type[bld:getSubtype()]
    elseif t == BT.SiegeEngine then return df.siegeengine_type[bld:getSubtype()]
    elseif t == BT.Construction then return df.construction_type[bld:getSubtype()]
    elseif t == BT.Civzone then return df.civzone_type[bld.type]
    end
  end) or ''
end
-- bld.jobs of a workshop/furnace as rf_core::BuildingJob list; the building's own construction/removal job is skipped
local JT_CONSTRUCT, JT_DESTROY = df.job_type.ConstructBuilding, df.job_type.DestroyBuilding
local function building_jobs(bld)
  local jobs = {}
  pcall(function()
    for _, j in ipairs(bld.jobs) do
      local jt = j.job_type
      if jt ~= JT_CONSTRUCT and jt ~= JT_DESTROY then
        local ok, s = pcall(function()
          local worker = try(dfhack.job.getWorker, j)
          return jobj(kv_append(job_core(jt, j.reaction_name, j.item_type, j.item_subtype, j.mat_type, j.mat_index,
                                         j.material_category),
            { 'repeat', jb(j.flags['repeat']), 'suspended', jb(j.flags.suspend), 'order_id', ji(j.order_id),
              'id', ji(j.id), 'worker', ji(worker and worker.id or -1), 'working', jb(j.flags.working),
              'do_now', jb(j.flags.do_now) }))
        end)
        if ok then jobs[#jobs + 1] = s else warn('building_job', s) end
      end
    end
  end)
  counts.building_jobs = (counts.building_jobs or 0) + #jobs
  return jobs
end
local PILE_CATS = { 'animals', 'food', 'furniture', 'corpses', 'refuse', 'stone', 'ammo', 'coins', 'bars_blocks', 'gems',
                    'finished_goods', 'leather', 'cloth', 'wood', 'weapons', 'armor', 'sheet' }
local function building_json(bld)
  local t = bld:getType()
  local stage = try(function() return bld:getBuildStage() end) or -1
  local maxst = try(function() return bld:getMaxBuildStage() end) or 0
  local owner = -1
  pcall(function() if bld.owner then owner = bld.owner.id end end)
  if owner == -1 then pcall(function() if bld.assigned_unit_id and bld.assigned_unit_id >= 0 then owner = bld.assigned_unit_id end end) end
  local stockpile = 'null'
  if t == BT.Stockpile then
    local cats = {}
    pcall(function()
      local fl = bld.settings.flags
      for _, c in ipairs(PILE_CATS) do if flag(fl, c) then cats[#cats + 1] = ja(c) end end
    end)
    local function mx(name)
      return try(function() return bld[name] end) or try(function() return bld.storage[name] end) or 0
    end
    stockpile = jobj({ 'categories', jarr(cats), 'max_barrels', ji(mx('max_barrels')), 'max_bins', ji(mx('max_bins')),
                       'max_wheelbarrows', ji(mx('max_wheelbarrows')) })
  end
  local ext = {}
  pcall(function()
    local r = bld.room
    if r and r.extents and r.width > 0 and r.x == bld.x1 and r.y == bld.y1
       and r.width == bld.x2 - bld.x1 + 1 and r.height == bld.y2 - bld.y1 + 1 then
      local full = true
      for k = 0, r.width * r.height - 1 do
        local v = (r.extents[k] ~= 0) and 1 or 0
        if v == 0 then full = false end
        ext[#ext + 1] = v == 1 and '1' or '0'
      end
      if full then ext = {} end
    end
  end)
  local items = {}
  pcall(function() for _, ci in ipairs(bld.contained_items) do items[#items + 1] = ji(ci.item.id) end end)
  local name = try(function() return bld.name end) or ''
  -- only the queues of workshops and furnaces (other buildings' bld.jobs hold hauling/planting jobs DF generates)
  local jobs = (t == BT.Workshop or t == BT.Furnace) and building_jobs(bld) or {}
  local crops = {}
  if t == BT.FarmPlot then
    for s = 0, 3 do
      local pid = try(function() return bld.plant_id[s] end) or -1
      crops[#crops + 1] = ja(pid >= 0 and (try(function() return raws.plants.all[pid].id end) or '') or '')
    end
    counts.farm_plots = (counts.farm_plots or 0) + 1
  end
  return jobj({ 'id', ji(bld.id), 'kind', ja(BT[t] or tostring(t)), 'subtype', ja(subtype_name(bld, t) or ''),
                'x1', ji(bld.x1), 'y1', ji(bld.y1), 'x2', ji(bld.x2), 'y2', ji(bld.y2), 'z', ji(bld.z),
                'center', jpos(bld.centerx, bld.centery, bld.z), 'mat', ji(palidx(bld.mat_type, bld.mat_index)),
                'stage', ji(stage), 'built', jb(stage >= maxst), 'owner', ji(owner), 'name', js(name),
                'stockpile', stockpile, 'extents', jarr(ext), 'items', jarr(items), 'jobs', jarr(jobs),
                'crops', jarr(crops) })
end

local function export_buildings()
  phase = 'buildings'
  local f = assert(io.open(OUT .. '/buildings.jsonl', 'wb'))
  local ids = id_snapshot(df.global.world.buildings.all)
  local n = 0
  for i = 1, #ids do
    local bld = df.building.find(ids[i])
    if bld then
      local ok, line = pcall(building_json, bld)
      if ok then f:write(line, '\n'); n = n + 1 else warn('building', line) end
    end
    if i % 16 == 0 then phase_detail = i .. '/' .. #ids; checkpoint() end
  end
  f:close()
  counts.buildings = n
end

-- ------------------------------------------------------------------------------------------- work orders
local function cond_json(c)
  local it = c.item_type
  local fl = {}
  for _, name in ipairs({ 'flags1', 'flags2', 'flags3' }) do
    for _, s in ipairs(set_flags(c[name], true)) do fl[#fl + 1] = s end
  end
  local function str(field) return try(function() return c[field] end) or '' end
  local function num(field) return try(function() return c[field] end) or -1 end
  if str('reaction_class') ~= '' then fl[#fl + 1] = js('REACTION_CLASS:' .. str('reaction_class')) end
  if str('has_material_reaction_product') ~= '' then
    fl[#fl + 1] = js('HAS_MATERIAL_REACTION_PRODUCT:' .. str('has_material_reaction_product'))
  end
  if num('metal_ore') >= 0 then fl[#fl + 1] = js('METAL_ORE:' .. mat_token(0, num('metal_ore')):gsub('^INORGANIC:', '')) end
  if num('has_tool_use') >= 0 then fl[#fl + 1] = ja('HAS_TOOL_USE:' .. (df.tool_uses[num('has_tool_use')] or num('has_tool_use'))) end
  if num('min_dimension') >= 0 then fl[#fl + 1] = ja('MIN_DIMENSION:' .. num('min_dimension')) end
  if num('reaction_id') >= 0 then fl[#fl + 1] = ja('REACTION_ID:' .. num('reaction_id')) end
  for _, k in ipairs({ 'flags4', 'flags5' }) do
    local v = try(function() return c[k] end)
    if type(v) == 'number' and v ~= 0 then fl[#fl + 1] = ja(('%s:0x%x'):format(k:upper(), v)) end
  end
  return jobj({ 'compare', ja(enum_name(c, 'compare_type')), 'value', ji(c.compare_val), 'item_type', ja(item_type_name(it)),
                'item_subtype', js(subtype_token(it, c.item_subtype)), 'mat', js(mat_token(c.mat_type, c.mat_index)),
                'flags', jarr(fl) })
end

local function order_json(o)
  local conds, oconds = {}, {}
  for _, c in ipairs(o.item_conditions) do conds[#conds + 1] = cond_json(c) end
  for _, c in ipairs(o.order_conditions) do
    oconds[#oconds + 1] = jobj({ 'order_id', ji(c.order_id), 'condition', ja(enum_name(c, 'condition')) })
  end
  return jobj(kv_append(job_core(o.job_type, o.reaction_name, o.item_type, o.item_subtype, o.mat_type, o.mat_index,
                                 o.material_category),
    { 'id', ji(o.id), 'amount_left', ji(o.amount_left), 'amount_total', ji(o.amount_total),
      'frequency', ja(enum_name(o, 'frequency')), 'workshop_id', ji(o.workshop_id),
      'active', jb(o.status.active), 'validated', jb(o.status.validated), 'conditions', jarr(conds),
      -- extras (not in rf_core::WorkOrder, ignored by serde)
      'order_conditions', jarr(oconds), 'max_workshops', ji(o.max_workshops),
      'finished_year', ji(o.finished_year), 'finished_year_tick', ji(o.finished_year_tick) }))
end

local function export_work_orders()
  phase = 'work_orders'
  -- manager_orders.all has no find(): it is small (tens of orders), so it is done in one slice without yielding
  local out = {}
  local all = df.global.world.manager_orders.all
  for i = 0, #all - 1 do
    local ok, s = pcall(order_json, all[i])
    if ok then out[#out + 1] = s else warn('work_order', s) end
  end
  write_file(OUT .. '/work_orders.jsonl', #out > 0 and (table.concat(out, '\n') .. '\n') or '')
  counts.work_orders = #out
  checkpoint()
end

-- ------------------------------------------------------------------------------------------- plant raws
local function mat_has_product(m, prod)
  local found = false
  pcall(function()
    for j = 0, #m.reaction_product.id - 1 do
      if m.reaction_product.id[j].value == prod then found = true return end
    end
  end)
  return found
end
local function plant_raw_json(p)
  local fl = p.flags
  local md = p.material_defs
  local function def(name)
    local mt = try(function() return md.type[name] end) or -1
    local mi = try(function() return md.idx[name] end) or -1
    return mt, mi
  end
  -- palette index (adds the material to the palette) of a material def; NO_MAT if the plant has none
  local function pal_of(name)
    local mt, mi = def(name)
    if mt < 0 then return 0xFFFF end
    return palidx(mt, mi)
  end
  local smat = nil
  do
    local mt, mi = def('basic_mat')
    local info = mt >= 0 and try(dfhack.matinfo.decode, mt, mi)
    smat = info and info.material or nil
  end
  local s_pal, d_pal, seed_pal = pal_of('basic_mat'), pal_of('drink'), pal_of('seed')
  local d_mt, seed_mt = def('drink'), def('seed')
  local mill_mt, mill_mi = def('mill')
  local thread_mt, thread_mi = def('thread')
  local xb_mt, xv_mt, xs_mt = def('extract_barrel'), def('extract_vial'), def('extract_still_vial')
  local biomes, under, above = {}, false, false
  pcall(function()
    for k, v in pairs(fl) do
      if v == true and type(k) == 'string' and k:match('^BIOME_') then
        local b = k:sub(7)
        biomes[#biomes + 1] = ja(b)
        if b:match('^SUBTERRANEAN') then under = true else above = true end
      end
    end
  end)
  local edible_raw = smat and flag(smat.flags, 'EDIBLE_RAW') or false
  local edible_cooked = smat and flag(smat.flags, 'EDIBLE_COOKED') or false
  local brew_from = (smat and mat_has_product(smat, 'DRINK_MAT')) and 'plant' or ''
  local growths = {}
  pcall(function()
    for _, g in ipairs(p.growths) do
      local info = try(dfhack.matinfo.decode, g.mat_type, g.mat_index)
      local gm = info and info.material
      local er = gm and flag(gm.flags, 'EDIBLE_RAW') or false
      local ec = gm and flag(gm.flags, 'EDIBLE_COOKED') or false
      local br = gm and mat_has_product(gm, 'DRINK_MAT') or false
      edible_raw = edible_raw or er
      edible_cooked = edible_cooked or ec
      if br and brew_from == '' then brew_from = 'growth' end
      growths[#growths + 1] = jobj({ 'id', ja(g.id), 'name', js(g.name), 'mat', js(mat_token(g.mat_type, g.mat_index)),
                                     'edible_raw', jb(er), 'edible_cooked', jb(ec), 'brewable', jb(br) })
    end
  end)
  local is_tree, is_grass = flag(fl, 'TREE'), flag(fl, 'GRASS')
  local gd = p.growdur
  return jobj({ 'token', ja(p.id), 'name', js(p.name), 'name_plural', js(p.name_plural),
                -- GROWDUR is in hundreds of ticks (wiki: 1008 GROWDUR units per season = 100800 ticks)
                'growdur', ji(gd * 100),
                'spring', jb(flag(fl, 'SPRING')), 'summer', jb(flag(fl, 'SUMMER')), 'autumn', jb(flag(fl, 'AUTUMN')),
                'winter', jb(flag(fl, 'WINTER')), 'underground', jb(under), 'aboveground', jb(above),
                'is_tree', jb(is_tree), 'is_shrub', jb(not is_tree and not is_grass),
                'brewable', jb(flag(fl, 'DRINK') or d_mt >= 0 or brew_from ~= ''),
                'edible_raw', jb(edible_raw), 'edible_cooked', jb(edible_cooked),
                'millable', jb(mill_mt >= 0), 'threshable', jb(thread_mt >= 0),
                'has_seed', jb(flag(fl, 'SEED') or seed_mt >= 0),
                'mat_structural', ji(s_pal), 'mat_drink', ji(d_pal), 'mat_seed', ji(seed_pal),
                -- extras (not in rf_core::PlantRaw, ignored by serde)
                'growdur_raw', ji(gd), 'value', ji(p.value), 'is_grass', jb(is_grass), 'biomes', jarr(biomes),
                'underground_depth', jarr({ ji(p.underground_depth_min), ji(p.underground_depth_max) }),
                'brew_from', ja(brew_from), 'mill', js(mill_mt >= 0 and mat_token(mill_mt, mill_mi) or ''),
                'thread', js(thread_mt >= 0 and mat_token(thread_mt, thread_mi) or ''),
                'extract_barrel', jb(xb_mt >= 0), 'extract_vial', jb(xv_mt >= 0), 'extract_still_vial', jb(xs_mt >= 0),
                'seed_name', js(p.seed_singular), 'growths', jarr(growths) })
end

local function export_plant_raws()
  phase = 'plant_raws'
  local out = {}
  local all = raws.plants.all   -- raws are static while a save is loaded: yielding is safe
  for i = 0, #all - 1 do
    local ok, s = pcall(plant_raw_json, all[i])
    if ok then out[#out + 1] = s else warn('plant_raw', s) end
    if i % 16 == 0 then phase_detail = i .. '/' .. #all; checkpoint() end
  end
  write_file(OUT .. '/plant_raws.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
  counts.plant_raws = #out
end

-- ------------------------------------------------------------------------------------------- burrows, squads
-- round 3: rf_core::Burrow with the tile list (from the block masks; bit x of bits[y] = local tile (x, y))
local function burrow_tile_list(b)
  local out, n = {}, 0
  for _, blk in ipairs(dfhack.burrows.listBlocks(b)) do
    local bx, by, bz = blk.map_pos.x, blk.map_pos.y, blk.map_pos.z
    local link = blk.block_burrows.next
    while link do
      local bb = link.item
      if bb and bb.id == b.id then
        for ly = 0, 15 do
          local bits = bb.tile_bitmask.bits[ly] & 0xFFFF
          if bits ~= 0 then
            for lx = 0, 15 do
              if bits & (1 << lx) ~= 0 then n = n + 1; out[n] = jpos(bx + lx, by + ly, bz) end
            end
          end
        end
      end
      link = link.next
    end
  end
  return out, n
end
local function find_burrow(id)
  for _, b in ipairs(df.global.plotinfo.burrows.list) do if b.id == id then return b end end
end
local function export_burrows()
  phase = 'burrows'
  local ids = {}
  pcall(function() for _, b in ipairs(df.global.plotinfo.burrows.list) do ids[#ids + 1] = b.id end end)
  local bl = {}
  local tiles_total = 0
  for i = 1, #ids do
    local b = find_burrow(ids[i])   -- re-found after every yield: the player may delete a burrow
    if b then
      local ok, s = pcall(function()
        local tiles, n = burrow_tile_list(b)
        tiles_total = tiles_total + n
        local units = {}
        for _, uid in ipairs(b.units) do units[#units + 1] = ji(uid) end
        return jobj({ 'id', ji(b.id), 'name', js(b.name), 'tiles', '[' .. table.concat(tiles, ',') .. ']', 'units', jarr(units),
                      -- extras
                      'tile_count', ji(n), 'blocks', ji(#b.block_x), 'flags', jarr(set_flags(b.flags, true)) })
      end)
      if ok then bl[#bl + 1] = s else warn('burrow', s) end
    end
    checkpoint()
  end
  write_file(OUT .. '/burrows.json', '[\n' .. table.concat(bl, ',\n') .. '\n]\n')
  counts.burrows = #bl
  counts.burrow_tiles = tiles_total
end

-- squad order as DF-like strings (rf_core::Squad::orders): Train, Station x,y,z, Kill <unit id>,
-- DefendBurrows <ids>, Patrol <route id>; other order types by their CamelCase type name
local function camel(s)
  return (tostring(s):lower():gsub('_(%l)', function(c) return c:upper() end):gsub('^%l', string.upper))
end
local function order_strings(o)
  local t = df.squad_order_type[o:getType()] or 'NONE'
  if t == 'TRAIN' then return { 'Train' } end
  if t == 'MOVE' then
    local p = o.pos
    return { ('Station %d,%d,%d'):format(p.x, p.y, p.z) }
  end
  if t == 'KILL_LIST' then
    local out = {}
    for _, uid in ipairs(o.units) do out[#out + 1] = 'Kill ' .. uid end
    if #out == 0 then out[1] = 'Kill' end
    return out
  end
  if t == 'DEFEND_BURROWS' then
    local b = {}
    for _, id in ipairs(o.burrows) do b[#b + 1] = tostring(id) end
    return { 'DefendBurrows ' .. table.concat(b, ',') }
  end
  if t == 'PATROL_ROUTE' then return { 'Patrol ' .. o.route_id } end
  return { camel(t) }
end
-- uniform entry: ITEM_TYPE[:SUBTYPE][@material token | @material class]
local function uniform_spec(sp)
  local s = item_type_name(sp.item_type)
  if s == '' then s = 'ANY' end
  local st = subtype_token(sp.item_type, sp.item_subtype)
  if st ~= '' then s = s .. ':' .. st end
  if sp.mattype >= 0 then s = s .. '@' .. mat_token(sp.mattype, sp.matindex)
  elseif sp.material_class >= 0 then s = s .. '@' .. (df.entity_material_category[sp.material_class] or sp.material_class) end
  return s
end
local function squad_json(s)
  local month = df.global.cur_year_tick // 33600
  local mem, hfs, uni_by_pos, pos_orders, seen_uni, uni = {}, {}, {}, {}, {}, {}
  for _, pos in ipairs(s.positions) do
    local uid = -1
    if pos.occupant >= 0 then
      local hf = df.historical_figure.find(pos.occupant)
      uid = hf and hf.unit_id or -1
    end
    mem[#mem + 1] = ji(uid)
    hfs[#hfs + 1] = ji(pos.occupant)
    local specs = {}
    pcall(function()
      local uf = pos.equipment.uniform
      for cat = 0, df.uniform_category._last_item do
        for _, sp in ipairs(uf[cat]) do
          local str = uniform_spec(sp)
          if not seen_uni[str] then seen_uni[str] = true; uni[#uni + 1] = js(str) end
          local asg = {}
          for _, iid in ipairs(sp.assigned) do asg[#asg + 1] = ji(iid) end
          specs[#specs + 1] = jobj({ 'category', ja(df.uniform_category[cat]), 'item', js(str), 'assigned', jarr(asg) })
        end
      end
    end)
    uni_by_pos[#uni_by_pos + 1] = jarr(specs)
    local po = {}
    pcall(function() for _, o in ipairs(pos.orders) do for _, x in ipairs(order_strings(o)) do po[#po + 1] = js(x) end end end)
    pos_orders[#pos_orders + 1] = jarr(po)
  end
  local explicit = {}
  pcall(function() for _, o in ipairs(s.orders) do for _, x in ipairs(order_strings(o)) do explicit[#explicit + 1] = js(x) end end end)
  -- the active routine and this month's scheduled orders
  local routine_name, sched, sched_entry = '', {}, 'null'
  pcall(function()
    for _, r in ipairs(df.global.plotinfo.alerts.routines) do if r.id == s.cur_routine_idx then routine_name = r.name end end
  end)
  pcall(function()
    local e = s.schedule.routine[s.cur_routine_idx].month[month]
    local so = {}
    for _, o in ipairs(e.orders) do
      for _, x in ipairs(order_strings(o.order)) do
        sched[#sched + 1] = js(x)
        so[#so + 1] = jobj({ 'order', js(x), 'min_count', ji(o.min_count) })
      end
    end
    sched_entry = jobj({ 'month', ji(month), 'name', js(e.name), 'sleep_mode', ji(e.sleep_mode), 'uniform_mode', ji(e.uniform_mode),
                         'orders', jarr(so) })
  end)
  -- current orders: the explicit squad orders if any, else this month's scheduled orders (DF behaviour)
  local current = #explicit > 0 and explicit or sched
  local rooms = {}
  pcall(function() for _, r in ipairs(s.rooms) do rooms[#rooms + 1] = ji(r.building_id) end end)
  local tname = try(dfhack.translation.translateName, s.name, true) or ''
  return jobj({ 'id', ji(s.id), 'name', js(tname), 'alias', js(s.alias), 'members', jarr(mem), 'orders', jarr(current),
                'uniform', jarr(uni), 'schedule', js(routine_name),
                -- extras
                'display_name', js(s.alias ~= '' and s.alias or tname),
                'name_native', js(try(dfhack.translation.translateName, s.name, false) or ''),
                'member_hfs', jarr(hfs), 'positions', ji(#s.positions), 'explicit_orders', jarr(explicit),
                'schedule_orders', jarr(sched), 'schedule_month', sched_entry, 'position_orders', jarr(pos_orders),
                'uniform_by_position', jarr(uni_by_pos), 'routine_id', ji(s.cur_routine_idx),
                'leader_position', ji(s.leader_position), 'barracks', jarr(rooms) })
end
local function export_squads()
  phase = 'squads'
  local gid = try(function() return df.global.plotinfo.group_id end) or -1
  local ids = {}
  pcall(function() for _, s in ipairs(df.global.world.squads.all) do if s.entity_id == gid then ids[#ids + 1] = s.id end end end)
  local sq = {}
  for i = 1, #ids do
    local s = df.squad.find(ids[i])
    if s then
      local ok, j = pcall(squad_json, s)
      if ok then sq[#sq + 1] = j else warn('squad', j) end
    end
    checkpoint()
  end
  write_file(OUT .. '/squads.json', '[\n' .. table.concat(sq, ',\n') .. '\n]\n')
  counts.squads = #sq
end
local function export_burrows_squads()
  export_burrows()
  export_squads()
end

-- ------------------------------------------------------------------------------------------- creature raws (round 3)
-- rf_core::CreatureRaw for the races on the map, the corpses on the map and the races of the world's civilizations.
-- Bodies come fully expanded from caste.body_info (DF did the BODY_DETAIL_PLAN work). DF keeps volumes in
-- units of 10 cm³; body sizes here are cm³ as in the raws (BODY_SIZE).
local ATTACK_FLAG_NAME = { with = 'WITH', latch = 'CANLATCH', edge = 'EDGE', independent_multiattack = 'INDEPENDENT_MULTIATTACK',
                           bad_multiattack = 'BAD_MULTIATTACK' }
local function str_vec0(v) return try(function() return v[0].value end) or '' end
local function creature_attack_json(a, parts)
  local fl = {}
  pcall(function()
    for k, v in pairs(a.flags) do
      if v == true and type(k) == 'string' and k ~= 'main' then fl[#fl + 1] = ja(ATTACK_FLAG_NAME[k] or k:upper()) end
    end
  end)
  pcall(function()
    for _, sa in ipairs(a.specialattack_type) do fl[#fl + 1] = ja('SPECIALATTACK_' .. (df.specialattack_type[sa] or sa)) end
  end)
  local bps, layers = {}, {}
  pcall(function() for _, i in ipairs(a.body_part_idx) do bps[#bps + 1] = ja(parts[i].token) end end)
  pcall(function() for _, i in ipairs(a.tissue_layer_idx) do layers[#layers + 1] = ji(i) end end)
  local first = try(function() return parts[a.body_part_idx[0]].token end) or ''
  return jobj({ 'name', ja(a.name), 'verb', js(a.verb_2nd), 'body_part', ja(first),
                -- creature attacks: ATTACK_CONTACT_PERC / ATTACK_PENETRATION_PERC (percent of the part)
                'contact_area', ji(a.contact_perc), 'penetration', ji(a.penetration_perc),
                'velocity_multiplier', ji(a.velocity_modifier), 'prepare_ticks', ji(a.time_to_strike),
                'recover_ticks', ji(a.time_to_recover), 'main', jb(a.flags.main), 'flags', jarr(fl),
                -- extras
                'verb_3rd', js(a.verb_3rd), 'skill', ja(df.job_skill[a.skill] or tostring(a.skill)), 'perc', 'true',
                'body_parts', jarr(bps), 'tissue_layers', jarr(layers) })
end
local function creature_raw_json(ri, source)
  local cr = raws.creatures.all[ri]
  -- tissues: name, material token, palette index, flags
  local tis = {}
  for ti, t in ipairs(cr.tissue) do
    local mt, mi = t.mat_type, t.mat_index
    tis[ti] = { id = t.id, tok = mat_token(mt, mi), pal = palidx(mt, mi), flags = set_flags(t.flags, true),
                shape = t.tissue_shape_str or '', thick = t.relative_thickness }
  end
  local castes = {}
  for ci, c in ipairs(cr.caste) do
    local bi = c.body_info
    local parts = bi.body_parts
    local pj = {}
    for _, p in ipairs(parts) do
      local layers = {}
      for _, l in ipairs(p.layers) do
        local t = tis[l.tissue_id] or { id = '', tok = '', pal = 0xFFFF, flags = {} }
        local fl, seen = {}, {}
        for _, s in ipairs(set_flags(l.flags, true)) do if not seen[s] then seen[s] = true; fl[#fl + 1] = s end end
        for _, s in ipairs(t.flags) do if not seen[s] then seen[s] = true; fl[#fl + 1] = s end end
        layers[#layers + 1] = jobj({ 'tissue', ja(l.layer_name), 'material', js(t.tok), 'mat', ji(t.pal),
                                     'thickness', ji(l.part_fraction), 'flags', jarr(fl),
                                     -- extras
                                     'tissue_id', ja(t.id), 'healing_rate', ji(l.healing_rate), 'vascular', ji(l.vascular),
                                     'pain_receptors', ji(l.pain_receptors), 'layer_depth', ji(l.layer_depth),
                                     'relative_thickness', ji(t.thick or 0) })
      end
      pj[#pj + 1] = jobj({ 'token', ja(p.token), 'name', js(str_vec0(p.name_singular)), 'category', ja(p.category),
                           'parent', ji(p.con_part_id), 'relsize', ji(p.relsize), 'flags', jarr(set_flags(p.flags, false)),
                           'layers', jarr(layers),
                           -- extras
                           'name_plural', js(str_vec0(p.name_plural)), 'number', ji(p.number),
                           'fraction_total', ji(p.fraction_total), 'fraction_fat', ji(p.fraction_fat),
                           'fraction_muscle', ji(p.fraction_muscle) })
      checkpoint()   -- raws are static while a save is loaded: yielding is safe
    end
    local attacks = {}
    for _, a in ipairs(bi.attacks) do
      local ok, s = pcall(creature_attack_json, a, parts)
      if ok then attacks[#attacks + 1] = s else warn('creature_attack', s) end
    end
    local sizes = {}
    local adult = 0
    pcall(function()
      for k = 0, #c.body_size - 1 do
        sizes[#sizes + 1] = jarr({ ji(c.body_size_day[k]), ji(c.body_size[k] * 10) })
        adult = c.body_size[k] * 10
      end
    end)
    castes[#castes + 1] = jobj({ 'token', ja(c.caste_id), 'name', js(c.caste_name[0]), 'body_size', ji(adult),
                                 'walk_speed', ji(caste_walk_speed(ri, ci, c) or 900), 'attacks', jarr(attacks),
                                 'body_parts', jarr(pj), 'flags', jarr(set_flags(c.flags, false)),
                                 -- extras: [age in days, cm³] per BODY_SIZE entry; total relsize; blood
                                 'body_size_by_age', jarr(sizes), 'total_relsize', ji(bi.total_relsize),
                                 'baby_age', ji(c.misc.baby_age), 'child_age', ji(c.misc.child_age),
                                 'max_age', jarr({ ji(c.misc.maxage_min), ji(c.misc.maxage_max) }) })
  end
  return jobj({ 'token', ja(cr.creature_id), 'name', js(cr.name[0]), 'name_plural', js(cr.name[1]), 'castes', jarr(castes),
                'flags', jarr(set_flags(cr.flags, false)),
                -- extras
                'index', ji(ri), 'source', jarr(source), 'tissues', ji(#cr.tissue) })
end
local function export_creature_raws()
  phase = 'creature_raws'
  local want, why = {}, {}
  local function mark(ri, s)
    if ri == nil or ri < 0 then return end
    if not want[ri] then want[ri] = true; why[ri] = {} end
    why[ri][s] = true
  end
  for ri in pairs(race_seen) do mark(ri, 'unit') end
  for ri in pairs(corpse_races) do mark(ri, 'item') end
  -- races of the world's civilizations (visitors, traders, invaders)
  pcall(function()
    for _, e in ipairs(df.global.world.entities.all) do
      if e.type == df.historical_entity_type.Civilization then mark(e.race, 'civ') end
    end
  end)
  local list = {}
  for ri in pairs(want) do list[#list + 1] = ri end
  table.sort(list)
  local out = {}
  for i, ri in ipairs(list) do
    phase_detail = i .. '/' .. #list
    local src = {}
    for s in pairs(why[ri]) do src[#src + 1] = ja(s) end
    table.sort(src)
    local ok, s = pcall(creature_raw_json, ri, src)
    if ok then out[#out + 1] = s else warn('creature_raw', s) end
    checkpoint()
  end
  write_file(OUT .. '/creature_raws.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
  counts.creature_raws = #out
end

-- ------------------------------------------------------------------------------------------- weapon / armor raws (round 3)
-- sizes in cm³ as in the raws (DF memory /10 -> x10 here); attack contact/penetration are absolute as in [ATTACK:...]
local function weapon_attack_json(a)
  local fl = set_flags(a.flags, true)
  if a.edged then fl[#fl + 1] = ja('EDGE') end
  return jobj({ 'name', js(a.verb_2nd), 'verb', js(a.verb_2nd), 'body_part', ja(''), 'contact_area', ji(a.contact),
                'penetration', ji(a.penetration), 'velocity_multiplier', ji(a.velocity_mult), 'prepare_ticks', ji(a.prepare),
                'recover_ticks', ji(a.recover), 'main', 'false', 'flags', jarr(fl),
                -- extras
                'verb_3rd', js(a.verb_3rd), 'noun', js(a.noun), 'edged', jb(a.edged), 'perc', 'false' })
end
local function attacks_of(d)
  local out = {}
  pcall(function() for _, a in ipairs(d.attacks) do out[#out + 1] = weapon_attack_json(a) end end)
  return out
end
local function weapon_raw_json(d, kind)
  local skill, skill_r = '', ''
  pcall(function() if d.skill_melee >= 0 then skill = df.job_skill[d.skill_melee] end end)
  pcall(function() if d.skill_ranged >= 0 then skill_r = df.job_skill[d.skill_ranged] end end)
  if skill == '' then skill = skill_r end
  local function n(f) return try(function() return d[f] end) or 0 end
  return jobj({ 'token', ja(d.id), 'name', js(d.name), 'skill', ja(skill), 'size', ji(n('size') * 10),
                'two_handed', ji(n('two_handed') * 10), 'minimum_size', ji(n('minimum_size') * 10),
                'ranged_ammo', js(try(function() return d.ranged_ammo end) or try(function() return d.ammo_class end) or ''),
                'attacks', jarr(attacks_of(d)),
                -- extras
                'kind', ja(kind), 'name_plural', js(d.name_plural), 'adjective', js(try(function() return d.adjective end) or ''),
                'skill_ranged', ja(skill_r), 'material_size', ji(n('material_size')), 'value', ji(n('value')),
                'shoot_force', ji(n('shoot_force')), 'shoot_maxvel', ji(n('shoot_maxvel')), 'hits', ji(n('hits')),
                'flags', jarr(set_flags(d.flags, false)) })
end
local ARMOR_LAYER = { [0] = 'UNDER', [1] = 'OVER', [2] = 'ARMOR', [3] = 'COVER' }
local function armor_raw_json(d, kind)
  local function n(f) return try(function() return d[f] end) or 0 end
  local layer, coverage, fl, lsize, lpermit = '', 100, {}, 0, 0
  pcall(function()
    local p = d.props
    layer = ARMOR_LAYER[p.layer] or tostring(p.layer)
    coverage, lsize, lpermit = p.coverage, p.layer_size, p.layer_permit
    fl = set_flags(p.flags, false)
  end)
  local dfl = try(function() return d.flags end)   -- shields have no flags
  if dfl then for _, s in ipairs(set_flags(dfl, false)) do fl[#fl + 1] = s end end
  return jobj({ 'token', ja(d.id), 'name', js(d.name), 'kind', ja(kind), 'layer', ja(layer), 'coverage', ji(coverage),
                'flags', jarr(fl),
                -- extras
                'name_plural', js(d.name_plural), 'armorlevel', ji(n('armorlevel')), 'ubstep', ji(n('ubstep')),
                'lbstep', ji(n('lbstep')), 'upstep', ji(n('upstep')), 'layer_size', ji(lsize), 'layer_permit', ji(lpermit),
                'material_size', ji(n('material_size')), 'value', ji(n('value')), 'blockchance', ji(n('blockchance')) })
end
local function export_item_raws()
  phase = 'item_raws'
  local W = raws.itemdefs
  local wo = {}
  for _, pair in ipairs({ { W.weapons, 'WEAPON' }, { W.ammo, 'AMMO' }, { W.trapcomps, 'TRAPCOMP' } }) do
    for _, d in ipairs(pair[1]) do
      local ok, s = pcall(weapon_raw_json, d, pair[2])
      if ok then wo[#wo + 1] = s else warn('weapon_raw', s) end
    end
    checkpoint()
  end
  write_file(OUT .. '/weapon_raws.json', '[\n' .. table.concat(wo, ',\n') .. '\n]\n')
  counts.weapon_raws = #wo
  local ao = {}
  for _, pair in ipairs({ { W.armor, 'ARMOR' }, { W.helms, 'HELM' }, { W.gloves, 'GLOVES' }, { W.shoes, 'SHOES' },
                          { W.pants, 'PANTS' }, { W.shields, 'SHIELD' } }) do
    for _, d in ipairs(pair[1]) do
      local ok, s = pcall(armor_raw_json, d, pair[2])
      if ok then ao[#ao + 1] = s else warn('armor_raw', s) end
    end
    checkpoint()
  end
  write_file(OUT .. '/armor_raws.json', '[\n' .. table.concat(ao, ',\n') .. '\n]\n')
  counts.armor_raws = #ao
end

-- ------------------------------------------------------------------------------------------- mods
-- active mods of the loaded save in load order (the raws object loader DF used for this save;
-- the same source DFHack's script-manager reads): "id:numeric_version"
local function mods_lists()
  local ids, detail = {}, {}
  pcall(function()
    local ol = df.global.world.object_loader
    for i = 0, #ol.object_load_order_id - 1 do
      local id = ol.object_load_order_id[i].value
      local nv = ol.object_load_order_numeric_version[i]
      ids[#ids + 1] = js(id .. ':' .. nv)
      detail[#detail + 1] = jobj({ 'id', js(id), 'name', js(try(function() return ol.object_load_order_name[i].value end) or ''),
                                   'version', js(try(function() return ol.object_load_order_displayed_version[i].value end) or ''),
                                   'numeric_version', ji(nv) })
    end
  end)
  return ids, detail
end

-- ------------------------------------------------------------------------------------------- main
local function main()
  phase = 'tiletypes'
  local tts, ntt = tiletypes_json()
  write_file(OUT .. '/tiletypes.json', tts)
  counts.tiletypes = ntt
  checkpoint()
  export_plants()          -- before the map: fills the tree/shrub index used for tile materials
  if not opt.skip_map then export_map() else note('map skipped (--skip-map)') end
  export_units()
  export_items()
  export_buildings()
  export_work_orders()
  export_plant_raws()      -- before the materials: adds the plants' STRUCTURAL/DRINK/SEED materials to the palette
  export_burrows_squads()
  export_creature_raws()   -- after units/items (races seen), before the materials: adds the tissue materials
  export_item_raws()
  phase = 'materials'
  local mats = {}
  local i = 1
  while i <= #pal do       -- palette may still grow? no: all palidx calls are done; loop by index anyway
    mats[#mats + 1] = material_json(pal[i][1], pal[i][2])
    if i % 8 == 0 then phase_detail = i .. '/' .. #pal; checkpoint() end
    i = i + 1
  end
  write_file(OUT .. '/materials.json', '[\n' .. table.concat(mats, ',\n') .. '\n]\n')
  counts.materials = #mats
  for k, v in pairs(unresolved) do note(('map: %d %s tiles without resolvable material'):format(v, k)) end
  for k, v in pairs(warn_count) do if v > 3 then note(('%s: %d errors in total'):format(k, v)) end end
  note(('walk_speed = full_speed of the caste default gait (body_info.gait_info, WALK; FLY for fliers): %d units from raws (%d flying), %d fell back to 900; attribute/injury/terrain modifiers not applied')
       :format(gait_stat.raw, gait_stat.fly, gait_stat.fallback))
  note('plant_raws: growdur = GROWDUR x 100 ticks (wiki: unit is hundreds of ticks, 1008 per season)')
  note('buildings: jobs only for Workshop/Furnace, without the ConstructBuilding/DestroyBuilding job; job/order item_type falls back to the job type product (df.job_type.attrs.item)')
  note('LAVA_STONE and MAGMA (semi-molten rock) tiles -> INORGANIC:OBSIDIAN (approx.); POOL/BROOK/RIVER tiles -> geo layer')
  note(('TREE/ROOT/MUSHROOM tiles: %d resolved exactly (getPlantAtTile), %d via nearest tree trunk (approx.)')
       :format(stat.tree_exact, stat.tree_nearest))
  note('GRASS tiles -> STRUCTURAL material of the grass event with the largest amount; PLANT tiles -> plant at the tile')
  note(('round 3 units: %d needs, %d emotions, %d values, %d relationships, %d wound entries, %d hostile units with an invader civ')
       :format(r3.needs, r3.emotions, r3.values, r3.relationships, r3.wounds, r3.invaders))
  note('relationships: family from histfig_links (+ siblings via the parents), friends/acquaintances from hf_visual: kind from love (Friend >= 50, CloseFriend >= 75, KindredSpirit 100, Grudge < 0 = estimate) and meet_count (LongTermAcquaintance >= 30)')
  note('wounds: one entry per wound layer; severity = surface_perc x 100 (estimate), raw DF values in the extras')
  note('volumes (unit body_size, caste body_size, weapon size/two_handed/minimum_size) are cm3 as in the raws = DF memory x 10')
end

local function finish()
  local T1 = now()
  local wd = df.global.world.world_data
  local mpz = {}
  pcall(function() mpz = { df.global.world.map.region_x, df.global.world.map.region_y, df.global.world.map.region_z } end)
  local cnt = {}
  counts.unit_needs, counts.unit_emotions, counts.unit_values = r3.needs, r3.emotions, r3.values
  counts.unit_relationships, counts.unit_wounds = r3.relationships, r3.wounds
  for _, k in ipairs({ 'units', 'citizens', 'items', 'buildings', 'plants', 'materials', 'tiletypes', 'tiles', 'z_exported',
                       'work_orders', 'plant_raws', 'building_jobs', 'farm_plots', 'burrows', 'squads',
                       'burrow_tiles', 'creature_raws', 'weapon_raws', 'armor_raws', 'unit_needs', 'unit_emotions',
                       'unit_values', 'unit_relationships', 'unit_wounds' }) do
    cnt[#cnt + 1] = k
    cnt[#cnt + 1] = ji(counts[k] or 0)
  end
  -- DF's stress category cutoffs (dfhack.units.getStressCutoffs), for the social simulation
  local cutoffs = {}
  pcall(function()
    local c = dfhack.units.getStressCutoffs()   -- 0-based list in 53.16-r2
    local i = (c[0] ~= nil) and 0 or 1
    while c[i] ~= nil and i < 32 do cutoffs[#cutoffs + 1] = ji(c[i]); i = i + 1 end
  end)
  local nl = {}
  for _, s in ipairs(notes) do nl[#nl + 1] = js(s) end
  local mods, mods_detail = mods_lists()
  local manifest = jobj({
    'format_version', ji(VERSION), 'exporter_rev', ja(SCRIPT_REV),
    'df_version', ja((dfhack.getDFVersion() or ''):match('0%.(%d+%.%d+)') or dfhack.getDFVersion() or ''), 'dfhack_version', ja(dfhack.getDFHackVersion()),
    'world_name', js(dfhack.translation.translateName(wd.name, false)),
    'world_name_english', js(dfhack.translation.translateName(wd.name, true)),
    'fort_name', js(FORT_EN), 'fort_name_native', js(FORT_NATIVE), 'site_id', ji(SITE_ID),
    'civ_id', ji(try(function() return df.global.plotinfo.civ_id end) or -1),
    'group_id', ji(try(function() return df.global.plotinfo.group_id end) or -1),
    'region_x', ji(mpz[1] or 0), 'region_y', ji(mpz[2] or 0), 'region_z', ji(mpz[3] or 0),
    'dims', jobj({ 'x', ji(XC), 'y', ji(YC), 'z', ji(ZC) }),
    'year', ji(START_YEAR), 'year_tick_start', ji(START_TICK), 'year_end', ji(df.global.cur_year),
    'year_tick_end', ji(df.global.cur_year_tick),
    'counts', jobj(cnt), 'export_seconds', jf((T1 - T0) / 1000), 'lua_seconds', jf(total_lua / 1000),
    'frames', ji(frames), 'max_slice_ms', ji(max_slice), 'budget_ms', ji(opt.budget),
    'map_complete', jb(not opt.skip_map and not opt.zmin and not opt.zmax),
    'mods', jarr(mods), 'mods_detail', '[\n  ' .. table.concat(mods_detail, ',\n  ') .. '\n]',
    'stress_cutoffs', jarr(cutoffs),
    'max_slice_by_phase', (function()
      local kv = {}
      for k, v in pairs(phase_max) do kv[#kv + 1] = '"' .. k .. '":' .. ji(v) end
      table.sort(kv)
      return '{' .. table.concat(kv, ',') .. '}'
    end)(),
    'notes', '[\n  ' .. table.concat(nl, ',\n  ') .. '\n]' })
  write_file(OUT .. '/manifest.json', manifest .. '\n')
  phase = 'done'
  progress(true)
  os.remove(LOCK)
  print(('rustfort-export: done in %.1f s (%d frames, max slice %d ms) -> %s'):format((T1 - T0) / 1000, frames, max_slice, OUT))
end

local function fail(err)
  local failed_phase = phase
  phase = 'error'
  phase_detail = tostring(err)
  pcall(write_file, OUT .. '/error.json', jobj({ 'error', js(tostring(err)), 'phase', ja(failed_phase) }) .. '\n')
  progress(true)
  os.remove(LOCK)
  dfhack.printerr('rustfort-export: aborted: ' .. tostring(err))
end

local co = coroutine.create(main)
local function step()
  local same = dfhack.isMapLoaded() and try(function() return df.global.world.world_data.active_site[0].id == SITE_ID end)
  if not same then
    fail('map unloaded or fort changed during export')
    return
  end
  local t = now()
  deadline = t + opt.budget
  local ph = phase
  local ok, err = coroutine.resume(co)
  local dt = now() - t
  frames = frames + 1
  total_lua = total_lua + dt
  if dt > max_slice then max_slice = dt end
  if dt > (phase_max[ph] or 0) then phase_max[ph] = dt end
  if not ok then fail(err) return end
  if coroutine.status(co) == 'dead' then
    local fok, ferr = pcall(finish)
    if not fok then fail(ferr) end
    return
  end
  dfhack.timeout(1, 'frames', step)
end

progress(true)
dfhack.timeout(1, 'frames', step)
print(('{"ok":true,"started":true,"fort":%s,"fort_native":%s,"out":%s,"budget_ms":%d}'):format(
  js(FORT_EN), js(FORT_NATIVE), ja(OUT), opt.budget))
