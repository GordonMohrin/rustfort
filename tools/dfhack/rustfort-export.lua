-- rustfort-export.lua - READ-ONLY exporter of the loaded DF fort into the Rustfort export format
-- (docs/EXPORT_FORMAT.md, format_version 1).  DF 53.16 + DFHack 53.16-r2.
--
--   dfhack-run lua -f <abs path>/rustfort-export.lua [<out_dir>] [options]
--     <out_dir>        where to write; default <DF folder>/rustfort-export/<native fort name, lower case>
--                      (letters and digits; other characters become _). The start line names the folder.
--     --budget MS      Lua time per game frame (default 25)
--     --expect NAME    only export if this native fort name is loaded (default: any fort)
--     --any-fort       no fort-name check (the default; kept for older callers)
--     --force          ignore a stale export.lock
--     --skip-map       only entities (units/items/buildings/plants) - for quick re-tests
--     --zmin N --zmax N  only export these z-levels of the map (testing; files of other z stay untouched)
--     --status         print progress.json of <out_dir> and exit
--     --cancel         ask a running export to stop (creates <out_dir>/cancel)
--
-- Safety contract (a live game the player keeps playing, possibly with their own scripts running):
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
--        + (round 4) entities.json nobles.json settings.json hist_figs.json caravans.json diplomats.json
--        + (round 5) engravings.json
--        + (round 8) work_details.json kitchen.json locations.json crimes.json punishments.json haul_routes.json
--        + (round 9) stockpile_options.json invasions.json

local VERSION = 1
-- round 2 (2026-10-10a): work orders, workshop jobs, crops, plant raws, mods, gaits
-- round 3 (2026-10-10b): needs/emotions/values/relationships/wounds/blood, creature/weapon/armor raws,
--                        material mechanics, squads with orders/uniforms/schedule, burrow tile lists,
--                        corpse creature tokens, heavy aquifer tile flag
-- round 4 (2026-10-10c): building flags/links/direction + trap contents, entities, nobles + mandates, settings,
--                        hist fig names, caravans, diplomats
-- round 5 (2026-10-10d): engravings.json (world.event.engravings)
-- round 8 (2026-10-10e): work details, kitchen, locations + occupations, crimes, haul routes, stockpile filters and
--                        links, animal fields (owner/training/pasture/slaughter/pregnancy), caste animal products,
--                        DF's room evaluation of civzones
-- round 9 (2026-10-10f): units DF does not simulate are inactive (caravan animals that left, units outside
--                        world.units.active), caged units take the cage item's position, chain/cage extras;
--                        squad ammo, per-position uniforms, patrol waypoints, supplies; DF's wealth statistics,
--                        invasion tiers and history (settings.other, invasions.json); stockpile_options.json
local SCRIPT_REV = '2026-10-10f'

-- ------------------------------------------------------------------------------------------- args
local args = { ... }
local opt = { budget = 25, expect = nil, force = false, skip_map = false }
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
-- Default output folder: <DF folder>/rustfort-export/<native fort name in lower case>.
local function fort_slug(name)
  local s = tostring(name or ''):lower():gsub('[^a-z0-9]', '_'):gsub('^_+', ''):gsub('_+$', '')
  return s ~= '' and s or 'fort'
end
local function default_out()
  if not dfhack.isMapLoaded() then return nil end
  local okn, native = pcall(function()
    return dfhack.translation.translateName(df.global.world.world_data.active_site[0].name, false)
  end)
  if not okn then return nil end
  local okp, dfpath = pcall(dfhack.getDFPath)
  dfpath = (okp and dfpath and dfpath ~= '') and tostring(dfpath) or '.'
  return dfpath:gsub('\\', '/'):gsub('/+$', '') .. '/rustfort-export/' .. fort_slug(native)
end
local OUT = opt.out or default_out()
if not OUT then
  print('{"ok":false,"error":"no output folder given and no fortress loaded"}')
  return
end
OUT = OUT:gsub('\\', '/'):gsub('/+$', '')

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
-- round 4: every historical figure referenced by units, relationships, deities, squads, nobles -> hist_figs.json
local HF_REF = {}
local function hf_ref(id) if id and id >= 0 then HF_REF[id] = true end end
-- round 4: entities seen on the map (invaders, merchants, visitors) -> entities.json
local CIV_SEEN = {}
local function needs_json(p)
  local out = {}
  for _, n in ipairs(p.needs) do
    hf_ref(n.deity_id)
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
    hf_ref(hfid)
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
-- ---- round 8 (rev 2026-10-10e): fortress management. All state and helpers live in this one table
-- (the main chunk is close to Lua's 200-locals limit); the functions are defined further down.
local r8 = { pets = 0, pastured = 0, slaughter = 0, pregnant = 0, trained = 0, wd_members = 0, pile_leaves = 0,
             piles = 0, wd_of = {}, pasture_of = {}, pile = {}, loc = {}, zone_loc = {} }
-- per-unit lookups, built once at the start of the units phase (no yields inside)
function r8.prepare_units()
  pcall(function()
    for _, d in ipairs(df.global.plotinfo.labor_info.work_details) do
      local nm = d.name
      for _, uid in ipairs(d.assigned_units) do
        local l = r8.wd_of[uid]
        if not l then l = {}; r8.wd_of[uid] = l end
        l[#l + 1] = nm
      end
    end
  end)
  -- pastures/pens: civzone assigned_units (DF v50: Pen/pasture zones list their animals there)
  pcall(function()
    local PEN = df.civzone_type.Pen
    for _, b in ipairs(df.global.world.buildings.all) do
      if b:getType() == df.building_type.Civzone and b.type == PEN then
        for _, uid in ipairs(b.assigned_units) do r8.pasture_of[uid] = b.id end
      end
    end
  end)
  -- round 9: which units DF simulates (world.units.active), which cage item / built cage holds a unit,
  -- which chain restrains it. Caravan animals that left with their caravan stay in units.all with
  -- flags1.forest, pos -30000 and inactive = false, but DF no longer simulates them.
  r8.active_set, r8.cage_of, r8.chain_of = {}, {}, {}
  r8.r9 = { offsite = 0, cage_pos = 0, chained = 0, caged = 0 }
  pcall(function() for _, u in ipairs(df.global.world.units.active) do r8.active_set[u.id] = true end end)
  pcall(function()
    for _, it in ipairs(df.global.world.items.other.CAGE) do
      for _, r in ipairs(it.general_refs) do
        if r:getType() == df.general_ref_type.CONTAINS_UNIT then
          local x, y, z = dfhack.items.getPosition(it)
          if type(x) == 'userdata' then x, y, z = x.x, x.y, x.z end
          local hb = try(dfhack.items.getHolderBuilding, it)
          r8.cage_of[r.unit_id] = { item = it.id, x = x or -30000, y = y or -30000, z = z or -30000, bld = hb and hb.id or -1 }
        end
      end
    end
  end)
  pcall(function()
    for _, b in ipairs(df.global.world.buildings.other.CHAIN) do
      local cu = try(function() return b.chained end)
      if cu then r8.chain_of[cu.id] = b.id end
    end
  end)
end
-- round 9: the position DF would draw a unit at (a caged unit inside a caravan's cage item keeps pos -30000)
function r8.unit_pos(u, count)
  local x, y, z, src = u.pos.x, u.pos.y, u.pos.z, 'unit'
  local c = r8.cage_of and r8.cage_of[u.id]
  if x < 0 and c and c.x >= 0 then x, y, z, src = c.x, c.y, c.z, 'cage_item'; if count then r8.r9.cage_pos = r8.r9.cage_pos + 1 end end
  if x < 0 then src = 'offsite' end
  return x, y, z, src
end
-- round 9: flags.inactive = DF does not simulate the unit (flags1.inactive, or not in world.units.active)
function r8.unit_inactive(u)
  local ina = not q(dfhack.units.isActive, u) or (r8.active_set ~= nil and next(r8.active_set) ~= nil and not r8.active_set[u.id])
  if ina and u.pos.x < 0 then r8.r9.offsite = r8.r9.offsite + 1 end
  return ina
end
-- rf_core::Unit round-8 fields: pet_owner, training, pasture, slaughter, pregnancy, work_details (+ extras)
function r8.unit_kv(u, is_animal)
  local owner = try(function() return u.relationship_ids.PetOwner end) or -1
  local training = ''
  if is_animal then training = try(function() return df.animal_training_level[u.training_level] end) or '' end
  local pasture = r8.pasture_of[u.id] or -1
  if pasture == -1 then
    pcall(function()
      for _, r in ipairs(u.general_refs) do
        if r:getType() == df.general_ref_type.BUILDING_CIVZONE_ASSIGNED then pasture = r.building_id break end
      end
    end)
  end
  local slaughter = try(function() return u.flags2.slaughter end) or false
  local preg = try(function() return u.pregnancy_timer end) or 0
  if owner >= 0 then r8.pets = r8.pets + 1 end
  if pasture >= 0 then r8.pastured = r8.pastured + 1 end
  if slaughter then r8.slaughter = r8.slaughter + 1 end
  if preg > 0 then r8.pregnant = r8.pregnant + 1 end
  if training ~= '' and training ~= 'WildUntamed' and training ~= 'Domesticated' then r8.trained = r8.trained + 1 end
  local c9 = r8.cage_of and r8.cage_of[u.id]
  local src9 = select(4, r8.unit_pos(u))
  if u.flags1.caged then r8.r9.caged = r8.r9.caged + 1 end
  if u.flags1.chained or (r8.chain_of and r8.chain_of[u.id]) then r8.r9.chained = r8.r9.chained + 1 end
  local wds = {}
  for _, nm in ipairs(r8.wd_of[u.id] or {}) do wds[#wds + 1] = js(nm) end
  r8.wd_members = r8.wd_members + #wds
  return { 'pet_owner', ji(owner), 'training', ja(training), 'pasture', ji(pasture), 'slaughter', jb(slaughter),
           'pregnancy', ji(preg), 'work_details', jarr(wds),
           -- extras
           'gelded', jb(try(function() return u.flags3.gelded end) or false),
           'only_do_assigned_jobs', jb(try(function() return u.flags4.only_do_assigned_jobs end) or false),
           'pregnancy_caste', ji(try(function() return u.pregnancy_caste end) or -1),
           'pregnancy_spouse', ji(try(function() return u.pregnancy_spouse end) or -1),
           -- round 9 extras: cage / restraint, where the position came from, DF's active list
           'chained', jb(u.flags1.chained or (r8.chain_of and r8.chain_of[u.id] ~= nil) or false),
           'chain_building', ji(r8.chain_of and r8.chain_of[u.id] or -1),
           'cage_item', ji(c9 and c9.item or -1), 'cage_building', ji(c9 and c9.bld or -1),
           'pos_source', ja(src9), 'in_active_list', jb(r8.active_set ~= nil and r8.active_set[u.id] == true) }
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
    'forest', jb(q(U.isForest, u)), 'inactive', jb(r8.unit_inactive(u)) })
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
  hf_ref(u.hist_figure_id)
  if u.civ_id >= 0 then CIV_SEEN[u.civ_id] = (CIV_SEEN[u.civ_id] or 0) + 1 end
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
    'pos', jpos(r8.unit_pos(u, true)), 'flags', flags, 'civ_id', ji(u.civ_id), 'hist_fig_id', ji(u.hist_figure_id),
    'sex', ji(u.sex), 'age', jf(try(U.getAge, u, true) or 0),
    'hunger_timer', ji(u.counters2.hunger_timer), 'thirst_timer', ji(u.counters2.thirst_timer),
    'sleepiness_timer', ji(u.counters2.sleepiness_timer), 'stress', ji(stress),
    'skills', jarr(skills), 'labors', jarr(labors), 'phys_attrs', jarr(phys), 'ment_attrs', jarr(ment),
    'facets', jarr(facets), 'inventory', jarr(inv), 'squad_id', ji(u.military.squad_id), 'mood', ja(mood),
    -- DF keeps volumes in units of 10 cm³ (raw BODY_SIZE / 10); body_size is cm³ since rev 2026-10-10b
    'body_size', ji(size_raw * 10), 'walk_speed', ji(walk_speed(u, caste)),
    'profession_name', js(try(U.getProfessionName, u) or ''), 'readable_name', js(try(U.getReadableName, u) or '') }
  for i = 1, #r3kv do kv[#kv + 1] = r3kv[i] end
  -- round 8: animals (owner, training, pasture, slaughter), pregnancy, work details
  local okr8, r8kv = pcall(r8.unit_kv, u, q(U.isAnimal, u))
  if okr8 then for i = 1, #r8kv do kv[#kv + 1] = r8kv[i] end else warn('unit_r8', r8kv) end
  return jobj(kv)
end

local function export_units()
  phase = 'units'
  local f = assert(io.open(OUT .. '/units.jsonl', 'wb'))
  r8.prepare_units()   -- round 8: work detail membership and pastures (one cheap pass, no yield)
  checkpoint()
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
-- ---- round 4: building states, mechanism links, trap contents
local r4 = { links = 0, levers = 0, locked_doors = 0, raised_bridges = 0, traps = 0, loaded_traps = 0, caged_units = 0,
             doors = 0, bridges = 0 }
local building_state   -- (defined in the block below; its helpers stay block-local: Lua allows 200 locals per chunk)
do
  -- DF 53.16 door_flags: forbidden (UI "Locked" = a player's "forbidden door"), unused_02 (was "internal"),
  -- taken_by_invaders, used_by_intruder, closed, operated_by_mechanisms; DF 53 has no pet-passable bit
  local DOOR_FLAG = { forbidden = 'LOCKED', unused_02 = 'INTERNAL', taken_by_invaders = 'TAKEN_BY_INVADERS',
                      used_by_intruder = 'USED_BY_INTRUDER', operated_by_mechanisms = 'OPERATED_BY_MECHANISMS' }
  -- building_bridgest.T_direction: Retracting -1, Left 0, Right 1, Up 2, Down 3 (map: up = north)
  local BRIDGE_DIR = { [-1] = 'RETRACT', [0] = 'W', [1] = 'E', [2] = 'N', [3] = 'S' }
  local GR = df.general_ref_type
  local REF_TRIGGER, REF_TARGET, REF_HOLDER = GR.BUILDING_TRIGGER, GR.BUILDING_TRIGGERTARGET, GR.BUILDING_HOLDER
  local REF_CONTAINS_UNIT = GR.CONTAINS_UNIT
  local IT = df.item_type
  -- names of the set bits of a building flag word: mapped (map) or uppercased; unnamed set bits as BIT<n>
  local function bits_upper(bf, map, out, nbits)
    pcall(function()
      for k, v in pairs(bf) do
        if type(k) == 'string' and v == true then
          if map and map[k] then out[#out + 1] = map[k]
          elseif not k:match('^unused') then out[#out + 1] = k:upper() end
        end
      end
    end)
    pcall(function()
      local w = bf.whole
      if type(w) == 'number' then
        for i = 0, (nbits or 16) - 1 do
          if w & (1 << i) ~= 0 and bf._type[i] == nil then out[#out + 1] = 'BIT' .. i end
        end
      end
    end)
    table.sort(out)
  end
  -- ids referenced by an item's general refs of one type (building, unit)
  local function item_refs(it, rt)
    local out = {}
    pcall(function()
      for _, r in ipairs(it.general_refs) do
        if r:getType() == rt then out[#out + 1] = (rt == REF_CONTAINS_UNIT) and r.unit_id or r.building_id end
      end
    end)
    return out
  end
  -- jobj key/value list: flags, links, direction (+ extras) of one building
  function building_state(bld, t)
    local fl, dir = {}, ''
    local targets, triggers, seen_t, seen_g = {}, {}, {}, {}
    local caged, trap_items = {}, {}
    local ext = {}
    local function add(s) fl[#fl + 1] = s end
    -- mechanism links: the mechanisms built into a building carry BUILDING_TRIGGERTARGET (in a lever/plate:
    -- the building it operates) or BUILDING_TRIGGER (in a target: the lever/plate that operates it)
    local n_cage, n_boulder, n_weapon = 0, 0, 0
    pcall(function()
      for _, ci in ipairs(bld.contained_items) do
        local it = ci.item
        for _, b in ipairs(item_refs(it, REF_TARGET)) do
          if b ~= bld.id and not seen_t[b] then seen_t[b] = true; targets[#targets + 1] = b end
        end
        for _, b in ipairs(item_refs(it, REF_TRIGGER)) do
          if b ~= bld.id and not seen_g[b] then seen_g[b] = true; triggers[#triggers + 1] = b end
        end
        local ity = it:getType()
        if ity == IT.CAGE then
          n_cage = n_cage + 1
          for _, uid in ipairs(item_refs(it, REF_CONTAINS_UNIT)) do caged[#caged + 1] = ji(uid) end
        elseif ity == IT.BOULDER then n_boulder = n_boulder + 1
        elseif ity == IT.WEAPON or ity == IT.TRAPCOMP then n_weapon = n_weapon + 1
        end
        if t == BT.Trap or t == BT.Weapon or ity == IT.CAGE then
          trap_items[#trap_items + 1] = jobj({ 'id', ji(it.id), 'kind', ja(IT[ity] or tostring(ity)),
            'subtype', js(subtype_token(ity, try(function() return it:getSubtype() end) or -1)), 'use_mode', ji(ci.use_mode) })
        end
      end
    end)
    -- levers/plates also list their mechanisms in linked_mechanisms (those items are held by the target building)
    pcall(function()
      for _, it in ipairs(bld.linked_mechanisms) do
        for _, b in ipairs(item_refs(it, REF_HOLDER)) do
          if b ~= bld.id and not seen_t[b] then seen_t[b] = true; targets[#targets + 1] = b end
        end
      end
    end)
    if t == BT.Door or t == BT.Hatch then
      r4.doors = r4.doors + 1
      local d = bld.door_flags
      bits_upper(d, DOOR_FLAG, fl, 16)
      for i = #fl, 1, -1 do if fl[i] == 'CLOSED' then table.remove(fl, i) end end
      add(d.closed and 'CLOSED' or 'OPEN')
      if d.forbidden then r4.locked_doors = r4.locked_doors + 1 end
      ext = { 'close_timer', ji(try(function() return bld.close_timer end) or 0) }
    elseif t == BT.Bridge then
      r4.bridges = r4.bridges + 1
      local g = bld.gate_flags
      bits_upper(g, nil, fl, 16)
      if not g.raised then add('LOWERED') else r4.raised_bridges = r4.raised_bridges + 1 end
      dir = BRIDGE_DIR[bld.direction] or tostring(bld.direction)
      ext = { 'timer', ji(bld.timer), 'material_amount', ji(try(function() return bld.material_amount end) or 0) }
    elseif t == BT.Floodgate or t == BT.GrateWall or t == BT.GrateFloor or t == BT.BarsVertical or t == BT.BarsFloor then
      pcall(function()
        local g = bld.gate_flags
        bits_upper(g, nil, fl, 16)
        if not g.closed then add('OPEN') end
      end)
    elseif t == BT.Weapon then
      pcall(function()
        local g = bld.gate_flags
        bits_upper(g, nil, fl, 16)
        if not g.retracted then add('EXTENDED') end
      end)
      if n_weapon > 0 then add('LOADED') end
    elseif t == BT.Trap then
      r4.traps = r4.traps + 1
      local tt = df.trap_type[bld.trap_type] or ''
      local ready = (try(function() return bld.ready_timeout end) or 0) == 0
      if not ready then add('RESETTING') end
      if tt == 'Lever' then
        r4.levers = r4.levers + 1
        if bld.state ~= 0 then add('ACTIVE') end
      elseif tt == 'PressurePlate' then
        local trig = {}
        pcall(function()
          local p = bld.plate_info
          for k, v in pairs(p.flags) do
            if v == true and type(k) == 'string' then
              add('PLATE_' .. k:upper())
              local lo = try(function() return p[k .. '_min'] end)
              if lo then trig[#trig + 1] = ('%s:%d-%d'):format(k, lo, p[k .. '_max']) end
            end
          end
        end)
        table.sort(trig)
        dir = table.concat(trig, ',')
        if ready then add('ACTIVE') end
        pcall(function()
          local p = bld.plate_info
          ext = { 'plate', jobj({ 'unit_min', ji(p.unit_min), 'unit_max', ji(p.unit_max), 'water_min', ji(p.water_min),
            'water_max', ji(p.water_max), 'magma_min', ji(p.magma_min), 'magma_max', ji(p.magma_max),
            'track_min', ji(p.track_min), 'track_max', ji(p.track_max) }) }
        end)
      elseif tt == 'TrackStop' then
        pcall(function()
          local ts = bld.track_stop_info
          bits_upper(ts.track_flags, nil, fl, 8)
          local dx, dy = ts.dump_x_shift, ts.dump_y_shift
          if dx ~= 0 or dy ~= 0 then dir = (dy < 0 and 'N' or dy > 0 and 'S' or '') .. (dx < 0 and 'W' or dx > 0 and 'E' or '') end
          ext = { 'friction', ji(ts.friction) }
        end)
      else
        -- armed = loaded and not resetting. Cage trap: an empty cage; stone-fall trap: a boulder;
        -- weapon trap: weapons / trap components
        local loaded = (tt == 'CageTrap' and n_cage > 0 and #caged == 0) or (tt == 'StoneFallTrap' and n_boulder > 0)
                       or (tt == 'WeaponTrap' and n_weapon > 0)
        if loaded then add('LOADED'); r4.loaded_traps = r4.loaded_traps + 1 end
        if tt == 'CageTrap' and #caged > 0 then add('FULL') end
        if loaded and ready then add('ACTIVE') end
      end
      local tk = { 'trap_state', ji(bld.state), 'ready_timeout', ji(bld.ready_timeout), 'fill_timer', ji(bld.fill_timer) }
      for i = 1, #tk do ext[#ext + 1] = tk[i] end
    elseif t == BT.Well then
      pcall(function() bits_upper(bld.well_flags, nil, fl, 16) end)
      ext = { 'bucket_z', ji(try(function() return bld.bucket_z end) or -1) }
    elseif t == BT.TradeDepot then
      pcall(function() bits_upper(bld.trade_flags, nil, fl, 32) end)
      if try(function() return bld.accessible end) then add('ACCESSIBLE') end
    elseif t == BT.Support then
      pcall(function() bits_upper(bld.support_flags, nil, fl, 16) end)
    end
    -- rf_core::Building::links: a trigger lists its targets, a target its triggers; both explicit in the extras
    local links = {}
    for _, b in ipairs(targets) do links[#links + 1] = b end
    for _, b in ipairs(triggers) do if not seen_t[b] then links[#links + 1] = b end end
    r4.links = r4.links + #targets
    local function jl(l) local o = {} for i = 1, #l do o[i] = ji(l[i]) end return jarr(o) end
    local fj = {}
    for i = 1, #fl do fj[i] = ja(fl[i]) end
    r4.caged_units = r4.caged_units + #caged
    local kv = { 'flags', jarr(fj), 'links', jl(links), 'direction', ja(dir),
                 -- extras
                 'link_targets', jl(targets), 'link_triggers', jl(triggers) }
    if #trap_items > 0 then kv[#kv + 1] = 'trap_items'; kv[#kv + 1] = jarr(trap_items) end
    if #caged > 0 then kv[#kv + 1] = 'caged_units'; kv[#kv + 1] = jarr(caged) end
    for i = 1, #ext do kv[#kv + 1] = ext[i] end
    return kv
  end
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
    -- round 8: detailed filters (scanned in the pile_filters phase before this one), links, take-from-links-only
    local p8 = r8.pile[bld.id] or {}
    local function link_ids(v) local o = {} pcall(function() for _, b in ipairs(v) do o[#o + 1] = ji(b.id) end end) return o end
    local give, take, give_ws, take_ws = {}, {}, {}, {}
    pcall(function()
      give, take = link_ids(bld.links.give_to_pile), link_ids(bld.links.take_from_pile)
      give_ws, take_ws = link_ids(bld.links.give_to_workshop), link_ids(bld.links.take_from_workshop)
    end)
    local give_all, take_all = {}, {}
    for _, x in ipairs(give) do give_all[#give_all + 1] = x end
    for _, x in ipairs(give_ws) do give_all[#give_all + 1] = x end
    for _, x in ipairs(take) do take_all[#take_all + 1] = x end
    for _, x in ipairs(take_ws) do take_all[#take_all + 1] = x end
    stockpile = jobj({ 'categories', jarr(cats), 'max_barrels', ji(mx('max_barrels')), 'max_bins', ji(mx('max_bins')),
                       'max_wheelbarrows', ji(mx('max_wheelbarrows')),
                       'filters', jarr(p8.filters or {}),
                       'use_links_only', jb(try(function() return bld.stockpile_flag.use_links_only end) or false),
                       'give_to', jarr(give_all), 'take_from', jarr(take_all),
                       -- extras (ignored by serde): links split by target kind, DF's pile number
                       'give_to_pile', jarr(give), 'take_from_pile', jarr(take), 'give_to_workshop', jarr(give_ws),
                       'take_from_workshop', jarr(take_ws), 'stockpile_number', ji(try(function() return bld.stockpile_number end) or -1),
                       'filters_complete', jb(p8.filters ~= nil) })
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
  -- round 4: state flags, mechanism links, bridge direction, trap contents
  local okst, state = pcall(building_state, bld, t)
  if not okst then warn('building_state', state); state = { 'flags', '[]', 'links', '[]', 'direction', '""' } end
  local crops = {}
  if t == BT.FarmPlot then
    for s = 0, 3 do
      local pid = try(function() return bld.plant_id[s] end) or -1
      crops[#crops + 1] = ja(pid >= 0 and (try(function() return raws.plants.all[pid].id end) or '') or '')
    end
    counts.farm_plots = (counts.farm_plots or 0) + 1
  end
  -- round 8: DF's room evaluation of civzones (extras room_value, room_name, ...)
  if t == BT.Civzone then
    local okr, rkv = pcall(r8.room_kv, bld, owner)
    if okr then for i = 1, #rkv do state[#state + 1] = rkv[i] end else warn('room_eval', rkv) end
  end
  return jobj({ 'id', ji(bld.id), 'kind', ja(BT[t] or tostring(t)), 'subtype', ja(subtype_name(bld, t) or ''),
                'x1', ji(bld.x1), 'y1', ji(bld.y1), 'x2', ji(bld.x2), 'y2', ji(bld.y2), 'z', ji(bld.z),
                'center', jpos(bld.centerx, bld.centery, bld.z), 'mat', ji(palidx(bld.mat_type, bld.mat_index)),
                'stage', ji(stage), 'built', jb(stage >= maxst), 'owner', ji(owner), 'name', js(name),
                'stockpile', stockpile, 'extents', jarr(ext), 'items', jarr(items), 'jobs', jarr(jobs),
                'crops', jarr(crops), table.unpack(state) })
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

-- ------------------------------------------------------------------------------------------- round 8
-- Stockpile filters (StockpileSettings.filters), DF's room evaluation of civzones, work_details.json,
-- kitchen.json, locations.json, crimes.json, haul_routes.json, punishments.json. Helpers stay in this
-- block or in the r8 table (200-locals limit of the main chunk).
do
  local MT = raws.mat_table
  local OC = df.organic_mat_category
  -- "other material" lists of the stockpile settings, in DF's order (names as DFHack's stockpiles plugin
  -- uses them; a list whose length differs from DF's vector falls back to '#<index>' tokens)
  local OTHER = {
    furniture = { 'WOOD', 'PLANT_CLOTH', 'BONE', 'TOOTH', 'HORN', 'PEARL', 'SHELL', 'LEATHER', 'SILK', 'AMBER', 'CORAL',
                  'GREEN_GLASS', 'CLEAR_GLASS', 'CRYSTAL_GLASS', 'YARN' },
    finished = { 'WOOD', 'PLANT_CLOTH', 'BONE', 'TOOTH', 'HORN', 'PEARL', 'SHELL', 'LEATHER', 'SILK', 'AMBER', 'CORAL',
                 'GREEN_GLASS', 'CLEAR_GLASS', 'CRYSTAL_GLASS', 'YARN', 'WAX' },
    weapons = { 'WOOD', 'PLANT_CLOTH', 'BONE', 'SHELL', 'LEATHER', 'SILK', 'GREEN_GLASS', 'CLEAR_GLASS', 'CRYSTAL_GLASS', 'YARN' },
    ammo = { 'WOOD', 'BONE' },
    bars = { 'COAL', 'POTASH', 'ASH', 'PEARLASH', 'SOAP' },
    blocks = { 'GREEN_GLASS', 'CLEAR_GLASS', 'CRYSTAL_GLASS', 'WOOD' },
  }
  -- token kinds: how the index of a settings vector becomes a leaf token
  local tok_cache = {}
  local function leaf_token(kind, arg, i)
    local ck = kind .. ':' .. tostring(arg) .. ':' .. i
    local v = tok_cache[ck]
    if v then return v end
    v = try(function()
      if kind == 'creature' then return raws.creatures.all[i].creature_id
      elseif kind == 'organic' then return mat_token(MT.organic_types[arg][i], MT.organic_indexes[arg][i])
      elseif kind == 'inorganic' then return 'INORGANIC:' .. raws.inorganics.all[i].id
      elseif kind == 'plant' then return 'PLANT:' .. raws.plants.all[i].id .. ':WOOD'
      elseif kind == 'item_type' then return df.item_type[i]
      elseif kind == 'furniture_type' then return df.furniture_type[i]
      elseif kind == 'itemdef' then return raws.itemdefs[arg][i].id
      elseif kind == 'builtin' then return mat_token(i, -1)
      elseif kind == 'color' then return raws.descriptors.colors[i].id
      elseif kind == 'quality' then return df.item_quality[i]
      elseif kind == 'list' then return arg[i + 1]
      end
    end)
    if v == nil or v == '' then v = '#' .. i end
    tok_cache[ck] = v
    return v
  end
  -- category -> list of { DF field (in settings[category] or settings[struct]), group name, token kind, arg, struct }
  local G = {
    animals = { { 'enabled', 'enabled', 'creature' }, opts = { 'empty_cages', 'empty_traps' } },
    food = { { 'meat', 'meat', 'organic', OC.Meat }, { 'fish', 'fish', 'organic', OC.Fish },
             { 'unprepared_fish', 'unprepared_fish', 'organic', OC.UnpreparedFish }, { 'egg', 'egg', 'organic', OC.Eggs },
             { 'plants', 'plants', 'organic', OC.Plants }, { 'drink_plant', 'drink_plant', 'organic', OC.PlantDrink },
             { 'drink_animal', 'drink_animal', 'organic', OC.CreatureDrink }, { 'cheese_plant', 'cheese_plant', 'organic', OC.PlantCheese },
             { 'cheese_animal', 'cheese_animal', 'organic', OC.CreatureCheese }, { 'seeds', 'seeds', 'organic', OC.Seed },
             { 'leaves', 'leaves', 'organic', OC.PlantGrowth }, { 'powder_plant', 'powder_plant', 'organic', OC.PlantPowder },
             { 'powder_creature', 'powder_creature', 'organic', OC.CreaturePowder }, { 'glob', 'glob', 'organic', OC.Glob },
             { 'glob_paste', 'glob_paste', 'organic', OC.Paste }, { 'glob_pressed', 'glob_pressed', 'organic', OC.Pressed },
             { 'liquid_plant', 'liquid_plant', 'organic', OC.PlantLiquid }, { 'liquid_animal', 'liquid_animal', 'organic', OC.CreatureLiquid },
             { 'liquid_misc', 'liquid_misc', 'organic', OC.MiscLiquid }, opts = { 'prepared_meals' } },
    furniture = { { 'type', 'type', 'furniture_type' }, { 'other_mats', 'other_mats', 'list', OTHER.furniture },
                  { 'mats', 'mats', 'inorganic' }, { 'quality_core', 'quality_core', 'quality' }, { 'quality_total', 'quality_total', 'quality' } },
    corpses = { { 'corpses', 'corpses', 'creature' } },
    refuse = { { 'type', 'type', 'item_type' }, { 'corpses', 'corpses', 'creature' }, { 'body_parts', 'body_parts', 'creature' },
               { 'skulls', 'skulls', 'creature' }, { 'bones', 'bones', 'creature' }, { 'hair', 'hair', 'creature' },
               { 'shells', 'shells', 'creature' }, { 'teeth', 'teeth', 'creature' }, { 'horns', 'horns', 'creature' },
               opts = { 'fresh_raw_hide', 'rotten_raw_hide' } },
    stone = { { 'mats', 'mats', 'inorganic' }, { 'mats', 'ore', 'inorganic', nil, 'ore' } },
    ammo = { { 'type', 'type', 'itemdef', 'ammo' }, { 'other_mats', 'other_mats', 'list', OTHER.ammo }, { 'mats', 'mats', 'inorganic' },
             { 'quality_core', 'quality_core', 'quality' }, { 'quality_total', 'quality_total', 'quality' } },
    coins = { { 'mats', 'mats', 'inorganic' } },
    bars_blocks = { { 'bars_other_mats', 'bars_other_mats', 'list', OTHER.bars }, { 'blocks_other_mats', 'blocks_other_mats', 'list', OTHER.blocks },
                    { 'bars_mats', 'bars_mats', 'inorganic' }, { 'blocks_mats', 'blocks_mats', 'inorganic' } },
    gems = { { 'rough_other_mats', 'rough_other_mats', 'builtin' }, { 'cut_other_mats', 'cut_other_mats', 'builtin' },
             { 'rough_mats', 'rough_mats', 'inorganic' }, { 'cut_mats', 'cut_mats', 'inorganic' } },
    finished_goods = { { 'type', 'type', 'item_type' }, { 'other_mats', 'other_mats', 'list', OTHER.finished }, { 'mats', 'mats', 'inorganic' },
                       { 'color', 'color', 'color' }, { 'quality_core', 'quality_core', 'quality' }, { 'quality_total', 'quality_total', 'quality' },
                       opts = { 'dyed', 'undyed' } },
    leather = { { 'mats', 'mats', 'organic', OC.Leather }, { 'color', 'color', 'color' }, opts = { 'dyed', 'undyed' } },
    cloth = { { 'thread_silk', 'thread_silk', 'organic', OC.Silk }, { 'thread_plant', 'thread_plant', 'organic', OC.PlantFiber },
              { 'thread_yarn', 'thread_yarn', 'organic', OC.Yarn }, { 'thread_metal', 'thread_metal', 'organic', OC.MetalThread },
              { 'cloth_silk', 'cloth_silk', 'organic', OC.Silk }, { 'cloth_plant', 'cloth_plant', 'organic', OC.PlantFiber },
              { 'cloth_yarn', 'cloth_yarn', 'organic', OC.Yarn }, { 'cloth_metal', 'cloth_metal', 'organic', OC.MetalThread },
              { 'color', 'color', 'color' }, opts = { 'dyed', 'undyed' } },
    wood = { { 'mats', 'mats', 'plant' } },
    weapons = { { 'weapon_type', 'weapon_type', 'itemdef', 'weapons' }, { 'trapcomp_type', 'trapcomp_type', 'itemdef', 'trapcomps' },
                { 'other_mats', 'other_mats', 'list', OTHER.weapons }, { 'mats', 'mats', 'inorganic' },
                { 'quality_core', 'quality_core', 'quality' }, { 'quality_total', 'quality_total', 'quality' }, opts = { 'usable', 'unusable' } },
    armor = { { 'body', 'body', 'itemdef', 'armor' }, { 'head', 'head', 'itemdef', 'helms' }, { 'feet', 'feet', 'itemdef', 'shoes' },
              { 'hands', 'hands', 'itemdef', 'gloves' }, { 'legs', 'legs', 'itemdef', 'pants' }, { 'shield', 'shield', 'itemdef', 'shields' },
              { 'other_mats', 'other_mats', 'list', OTHER.weapons }, { 'mats', 'mats', 'inorganic' }, { 'color', 'color', 'color' },
              { 'quality_core', 'quality_core', 'quality' }, { 'quality_total', 'quality_total', 'quality' },
              opts = { 'usable', 'unusable', 'dyed', 'undyed' } },
    sheet = { { 'paper', 'paper', 'organic', OC.Paper }, { 'parchment', 'parchment', 'organic', OC.Parchment } },
  }
  local CAT_ORDER = { 'animals', 'food', 'furniture', 'corpses', 'refuse', 'stone', 'ammo', 'coins', 'bars_blocks', 'gems',
                      'finished_goods', 'leather', 'cloth', 'wood', 'weapons', 'armor', 'sheet' }
  r8.other_lists = OTHER
  r8.list_mismatch = {}

  -- read one settings vector of pile `bid`: returns the set indices and the length (re-finds the pile after a
  -- yield: DF may resize or free the vector between frames; a changed length aborts this group)
  local function scan(bid, cat, gr)
    local function vec()
      local b = df.building.find(bid)
      if not b then return nil end
      local s = b.settings
      local st = s[gr[5] or cat]
      return st and st[gr[1]]
    end
    local v = vec()
    if v == nil then return nil end
    local n
    if gr[3] == 'quality' then n = 7 else n = #v end
    local on = {}
    local i = 0
    while i < n do
      local x = v[i]
      if x == true or (type(x) == 'number' and x ~= 0) then on[#on + 1] = i end
      i = i + 1
      if i % 1024 == 0 and checkpoint() then
        v = vec()
        if v == nil or (gr[3] ~= 'quality' and #v ~= n) then return nil end
      end
    end
    return on, n
  end

  -- pile_filters phase: rf_core::StockpileSettings::filters per stockpile, before the buildings phase
  function r8.export_pile_filters()
    phase = 'pile_filters'
    local ids = {}
    for _, b in ipairs(df.global.world.buildings.all) do
      if b:getType() == df.building_type.Stockpile then ids[#ids + 1] = b.id end
    end
    for pi = 1, #ids do
      local bid = ids[pi]
      local ok, err = pcall(function()
        local b = df.building.find(bid)
        if not b then return end
        local s = b.settings
        local enabled, opts = {}, {}
        for _, c in ipairs(CAT_ORDER) do
          if flag(s.flags, c) then
            enabled[#enabled + 1] = c
            local ol = {}
            for _, o in ipairs(G[c].opts or {}) do if try(function() return s[c][o] end) then ol[#ol + 1] = o end end
            opts[c] = ol
          end
        end
        local misc = {}
        if try(function() return s.misc.allow_organic end) then misc[#misc + 1] = 'allow_organic' end
        if try(function() return s.misc.allow_inorganic end) then misc[#misc + 1] = 'allow_inorganic' end
        local out = {}
        local function add(str) out[#out + 1] = js(str) end
        for _, c in ipairs(enabled) do
          local before = #out
          for _, o in ipairs(opts[c]) do add(c .. '/options/' .. o) end
          for _, gr in ipairs(G[c]) do
            local on, n = scan(bid, c, gr)
            if on and n > 0 then
              if gr[3] == 'list' and #gr[4] ~= n then r8.list_mismatch[c .. '/' .. gr[2]] = n end
              local arg = gr[4]
              if gr[3] == 'list' and #gr[4] ~= n then arg = {} end
              local prefix = c .. '/' .. gr[2] .. '/'
              if #on == n then
                add(prefix .. '*')
              elseif #on * 2 > n then
                add(prefix .. '*')
                local k = 1
                for i = 0, n - 1 do
                  if on[k] == i then k = k + 1 else add(prefix .. '!' .. leaf_token(gr[3], arg, i)) end
                end
              else
                for _, i in ipairs(on) do add(prefix .. leaf_token(gr[3], arg, i)) end
              end
            end
            checkpoint()
          end
          if #out == before then add(c .. '/none/-') end
        end
        for _, m in ipairs(misc) do add('misc/options/' .. m) end
        if #misc == 0 then add('misc/none/-') end   -- neither organic nor inorganic allowed
        r8.pile[bid] = { filters = out }
        r8.piles = r8.piles + 1
        r8.pile_leaves = r8.pile_leaves + #out
      end)
      if not ok then warn('pile_filters', err) end
      phase_detail = pi .. '/' .. #ids
      checkpoint()
    end
  end

  -- round 9: stockpile_options.json - every leaf DF's stockpile editor offers, per category and group, in the
  -- same category/group/token scheme as the filters above. Which raw entries a group lists follows DFHack's
  -- stockpiles plugin (metal/stone/gem material rules, tree woods, finished-goods and refuse item types);
  -- organic groups list their whole organic material table, creature groups every creature.
  local FINISHED_TYPES = {}
  for _, t in ipairs({ 'CHAIN', 'FLASK', 'GOBLET', 'INSTRUMENT', 'TOY', 'ARMOR', 'SHOES', 'HELM', 'GLOVES', 'FIGURINE', 'AMULET',
                       'SCEPTER', 'CROWN', 'RING', 'EARRING', 'BRACELET', 'GEM', 'LARGE_GEM', 'TOTEM', 'PANTS', 'BACKPACK',
                       'QUIVER', 'SPLINT', 'CRUTCH', 'TOOL', 'BOOK' }) do FINISHED_TYPES[t] = true end
  local REFUSE_NOT = {}
  for _, t in ipairs({ 'NONE', 'BAR', 'SMALLGEM', 'BLOCKS', 'ROUGH', 'BOULDER', 'CORPSE', 'CORPSEPIECE', 'ROCK', 'ORTHOPEDIC_CAST' }) do
    REFUSE_NOT[t] = true
  end
  local MAT_RULE = { ['stone/mats'] = 'stone', ['stone/ore'] = 'ore', ['ammo/mats'] = 'metal', ['coins/mats'] = 'metal',
                     ['bars_blocks/bars_mats'] = 'metal', ['bars_blocks/blocks_mats'] = 'metal_stone', ['gems/rough_mats'] = 'gem',
                     ['gems/cut_mats'] = 'gem_stone', ['finished_goods/mats'] = 'any_hard', ['furniture/mats'] = 'metal_stone',
                     ['weapons/mats'] = 'metal', ['armor/mats'] = 'metal' }
  local function pretty_tok(s) return (tostring(s):lower():gsub('_', ' ')) end
  -- (count, allowed(i), label(i)) of one group
  local function group_source(cat, gr)
    local kind, arg = gr[3], gr[4]
    if kind == 'creature' then
      return #raws.creatures.all, function(i)
        local c = raws.creatures.all[i]
        return not c.flags.DOES_NOT_EXIST and c.creature_id ~= 'EQUIPMENT_WAGON'
      end, function(i) return raws.creatures.all[i].name[0] end
    elseif kind == 'organic' then
      return #MT.organic_types[arg], function() return true end, function(i)
        local info = dfhack.matinfo.decode(MT.organic_types[arg][i], MT.organic_indexes[arg][i])
        return info and info:toString() or ''
      end
    elseif kind == 'inorganic' then
      local rule = MAT_RULE[cat .. '/' .. gr[2]] or 'any'
      return #raws.inorganics.all, function(i)
        local f = raws.inorganics.all[i].material.flags
        local metal, gem = f.IS_METAL, f.IS_GEM
        local stone = f.IS_STONE and not f.NO_STONE_STOCKPILE
        if rule == 'metal' then return metal
        elseif rule == 'stone' then return stone
        elseif rule == 'ore' then return stone and #raws.inorganics.all[i].metal_ore.mat_index > 0
        elseif rule == 'gem' then return gem
        elseif rule == 'gem_stone' then return gem or stone
        elseif rule == 'metal_stone' then return metal or stone
        elseif rule == 'any_hard' then return metal or stone or gem
        end
        return true
      end, function(i) return raws.inorganics.all[i].material.state_name.Solid end
    elseif kind == 'plant' then
      return #raws.plants.all, function(i) return raws.plants.all[i].flags.TREE end,
             function(i) return raws.plants.all[i].name end
    elseif kind == 'item_type' then
      return df.item_type._last_item + 1, function(i)
        local t = df.item_type[i]
        if not t then return false end
        if cat == 'finished_goods' then return FINISHED_TYPES[t] == true end
        return not REFUSE_NOT[t]
      end, function(i) return pretty_tok(df.item_type[i]) end
    elseif kind == 'furniture_type' then
      return df.furniture_type._last_item + 1, function(i) return df.furniture_type[i] ~= nil end,
             function(i) return pretty_tok(df.furniture_type[i]) end
    elseif kind == 'itemdef' then
      return #raws.itemdefs[arg], function() return true end, function(i) return raws.itemdefs[arg][i].name end
    elseif kind == 'builtin' then
      return #MT.builtin, function(i)
        local m = MT.builtin[i]
        return m ~= nil and tostring(m.id or ''):match('GLASS') ~= nil
      end, function(i) return MT.builtin[i].state_name.Solid end
    elseif kind == 'color' then
      return #raws.descriptors.colors, function() return true end, function(i) return raws.descriptors.colors[i].name end
    elseif kind == 'quality' then
      return 7, function() return true end, function(i) return df.item_quality[i] end
    elseif kind == 'list' then
      return #arg, function() return true end, function(i) return pretty_tok(arg[i + 1]) end
    end
    return 0, function() return false end, function() return '' end
  end
  function r8.export_pile_options()
    phase = 'pile_options'
    local cats, total = {}, 0
    for _, c in ipairs(CAT_ORDER) do
      local groups = {}
      for _, gr in ipairs(G[c]) do
        local leaves = {}
        local ok, err = pcall(function()
          local n, allowed, label = group_source(c, gr)
          local arg = gr[4]
          for i = 0, n - 1 do
            if try(allowed, i) then
              local tok = leaf_token(gr[3], arg, i)
              local lb = try(label, i) or ''
              if lb == '' then lb = tok end
              leaves[#leaves + 1] = '[' .. js(tok) .. ',' .. js(lb) .. ']'
            end
            if i % 64 == 63 then phase_detail = c .. '/' .. gr[2] .. ' ' .. i .. '/' .. n; checkpoint() end
          end
        end)
        if not ok then warn('pile_options', err) end
        total = total + #leaves
        groups[#groups + 1] = jobj({ 'group', ja(gr[2]), 'leaves', '[' .. table.concat(leaves, ',') .. ']' })
        checkpoint()
      end
      local opts = {}
      for _, o in ipairs(G[c].opts or {}) do opts[#opts + 1] = ja(o) end
      cats[#cats + 1] = jobj({ 'category', ja(c), 'options', jarr(opts), 'groups', jarr(groups) })
    end
    write_file(OUT .. '/stockpile_options.json', jobj({
      'scheme', ja('category/group/token'), 'misc', jarr({ ja('allow_organic'), ja('allow_inorganic') }),
      'categories', '[\n' .. table.concat(cats, ',\n') .. '\n]',
      'source', ja('raws (creatures, inorganics, plants, itemdefs, colors), world.raws.mat_table.organic_types, df.item_type/furniture_type; inclusion rules as DFHack stockpiles plugin (estimate)') }) .. '\n')
    counts.stockpile_options = total
  end

  -- locations (abstract buildings of the fort's site) and which civzones belong to them
  local LOC_KIND = { TEMPLE = 'Temple', INN_TAVERN = 'Tavern', LIBRARY = 'Library', GUILDHALL = 'Guildhall', HOSPITAL = 'Hospital' }
  local function loc_tier_name(kind, value)
    local d = df.global.plotinfo.main.custom_difficulty
    -- DF's thresholds from the difficulty settings (same rule as DFHack's caravan/pedestal.lua)
    if kind == 'Temple' then
      local tc, tv = try(function() return d.temple_complex_value end), try(function() return d.temple_value end)
      if tc and value >= tc then return 'Temple Complex' end
      if tv and value >= tv then return 'Temple' end
      return 'Meeting Hall'
    elseif kind == 'Guildhall' then
      local gg, gv = try(function() return d.grand_guildhall_value end), try(function() return d.guildhall_value end)
      if gg and value >= gg then return 'Grand Guildhall' end
      if gv and value >= gv then return 'Guildhall' end
      return 'Meeting Hall'
    end
    return kind
  end
  function r8.prepare_locations()
    pcall(function()
      local site = df.global.plotinfo.main.fortress_site
      for _, ab in ipairs(site.buildings) do
        local ty = df.abstract_building_type[ab:getType()] or tostring(ab:getType())
        local kind = LOC_KIND[ty] or (ty:lower():gsub('_(%l)', string.upper):gsub('^%l', string.upper))
        local value = try(function() return ab.contents.location_value end) or 0
        local L = { id = ab.id, kind = kind, type = ty, value = value, tier = try(function() return ab.contents.location_tier end) or -1,
                    tier_name = loc_tier_name(kind, value), zones = {} }
        r8.loc[ab.id] = L
        pcall(function() for _, z in ipairs(ab.contents.building_ids) do r8.zone_loc[z] = L; L.zones[#L.zones + 1] = z end end)
      end
    end)
    pcall(function()
      for _, b in ipairs(df.global.world.buildings.all) do
        if b:getType() == df.building_type.Civzone and b.location_id >= 0 then
          local L = r8.loc[b.location_id]
          if L and r8.zone_loc[b.id] ~= L then r8.zone_loc[b.id] = L; L.zones[#L.zones + 1] = b.id end
        end
      end
    end)
  end

  -- DF's own evaluation of a civzone room: location value/tier for location zones; for personal rooms DF 53.16
  -- keeps no value (DFHack's getRoomDescription returns '' for v50 zones), so the owner's latest quality
  -- thought is written next to it.
  local ROOM_THOUGHT = { Bedroom = 'BedroomQuality', DiningHall = 'DiningQuality', Office = 'OfficeQuality', Tomb = 'TombQuality' }
  local PERSONAL = { Bedroom = true, DiningHall = true, Office = true, Tomb = true, Dormitory = true, Barracks = true }
  function r8.room_kv(bld, owner)
    local L = r8.zone_loc[bld.id]
    local ou = owner >= 0 and df.unit.find(owner) or nil
    local desc = try(dfhack.buildings.getRoomDescription, bld, ou) or ''
    local value, name = -1, desc
    local zt = df.civzone_type[bld.type] or ''
    -- the location value belongs to its meeting-area zone, not to rented tavern bedrooms or other personal rooms
    if L and not PERSONAL[zt] then value = L.value; if name == '' then name = L.tier_name end end
    local th, sev, year, tick = ROOM_THOUGHT[zt] or '', -1, -1, -1
    if th ~= '' and ou then
      pcall(function()
        local T = df.unit_thought_type[th]
        for _, e in ipairs(ou.status.current_soul.personality.emotions) do
          if e.thought == T and (e.year > year or (e.year == year and e.year_tick >= tick)) then
            sev, year, tick = e.severity, e.year, e.year_tick
          end
        end
      end)
    end
    if sev < 0 then th = '' end
    return { 'room_value', ji(value), 'room_name', js(name), 'room_description', js(desc),
             'location_id', ji(L and L.id or (try(function() return bld.location_id end) or -1)),
             'location_kind', ja(L and L.kind or ''), 'location_tier', ji(L and L.tier or -1),
             'room_thought', ja(th), 'room_thought_severity', ji(sev), 'room_thought_year', ji(year),
             'room_thought_tick', ji(tick), 'room_collision', jb(try(function() return bld.flags.room_collision end) or false) }
  end

  -- work_details.json, kitchen.json, locations.json, crimes.json, haul_routes.json, punishments.json
  local function export_work_details()
    phase = 'work_details'
    local out = {}
    pcall(function()
      for i, d in ipairs(df.global.plotinfo.labor_info.work_details) do
        local labors, units = {}, {}
        for l = LABOR_FIRST, LABOR_LAST do
          if l >= 0 and try(function() return d.allowed_labors[l] end) then labors[#labors + 1] = ja(df.unit_labor[l]) end
        end
        for _, uid in ipairs(d.assigned_units) do units[#units + 1] = ji(uid) end
        local mode = try(function() return df.work_detail_mode[d.flags.mode] end) or ''
        out[#out + 1] = jobj({ 'name', js(d.name), 'labors', jarr(labors), 'assigned', jarr(units), 'mode', ja(mode),
                               'icon', ja(tostring(d.icon)), 'no_other_work', 'false',
                               -- extras
                               'index', ji(i), 'no_modify', jb(d.flags.no_modify), 'cannot_be_everybody', jb(d.flags.cannot_be_everybody) })
      end
    end)
    write_file(OUT .. '/work_details.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
    counts.work_details = #out
    checkpoint()
  end

  local function export_kitchen()
    phase = 'kitchen'
    local out = {}
    pcall(function()
      local k = df.global.plotinfo.kitchen
      for i = 0, #k.item_types - 1 do
        local it, st, mt, mi = k.item_types[i], k.item_subtypes[i], k.mat_types[i], k.mat_indices[i]
        local ex = k.exc_types[i]
        out[#out + 1] = jobj({ 'item_type', ja(item_type_name(it)), 'item_subtype', js(subtype_token(it, st)),
                               'mat', js(mat_token(mt, mi)), 'no_cook', jb(ex.Cook), 'no_brew', jb(ex.Brew),
                               -- extras
                               'mat_type', ji(mt), 'mat_index', ji(mi), 'item_subtype_index', ji(st) })
      end
    end)
    write_file(OUT .. '/kitchen.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
    counts.kitchen = #out
  end

  local function export_locations()
    phase = 'locations'
    local out, occs = {}, 0
    pcall(function()
      local site = df.global.plotinfo.main.fortress_site
      for _, ab in ipairs(site.buildings) do
        local ok, s = pcall(function()
          local L = r8.loc[ab.id] or { kind = '', type = '', value = 0, tier = -1, tier_name = '', zones = {} }
          local deity, religion, dtype = -1, -1, -1
          pcall(function()
            dtype = ab.deity_type
            if dtype == 0 then deity = ab.deity_data.Deity elseif dtype == 1 then religion = ab.deity_data.Religion end
          end)
          hf_ref(deity)
          local prof = try(function() local p = ab.contents.profession return p >= 0 and df.profession[p] or '' end) or ''
          local o, od = {}, {}
          for _, oc in ipairs(ab.occupations) do
            local kind = df.occupation_type[oc.type] or tostring(oc.type)
            o[#o + 1] = jobj({ 'kind', ja(kind), 'unit', ji(oc.unit_id) })
            od[#od + 1] = jobj({ 'id', ji(oc.id), 'kind', ja(kind), 'unit', ji(oc.unit_id), 'histfig', ji(oc.histfig_id) })
            hf_ref(oc.histfig_id)
          end
          occs = occs + #o
          local fl = {}
          for k = 0, 31 do
            if try(function() return ab.flags[k] end) then fl[#fl + 1] = ja(df.abstract_building_flags[k] or ('BIT' .. k)) end
          end
          local function fset(n) return try(function() return ab.flags[df.abstract_building_flags[n]] end) or false end
          local zones = {}
          for _, z in ipairs(L.zones) do zones[#zones + 1] = ji(z) end
          local c = ab.contents
          local need = {}
          for _, nm in ipairs({ 'goblets', 'instruments', 'paper', 'splints', 'thread', 'cloth', 'crutches', 'powder', 'buckets', 'soap', 'copies' }) do
            local want = try(function() return c['desired_' .. nm] end) or 0
            local have = try(function() return c['count_' .. nm] end)
            if want > 0 or (have and have > 0) then need[#need + 1] = jobj({ 'item', ja(nm), 'desired', ji(want), 'count', ji(have or 0) }) end
          end
          return jobj({ 'id', ji(ab.id), 'kind', ja(L.kind), 'name', js(try(dfhack.translation.translateName, ab.name, true) or ''),
                        'deity', ji(deity), 'profession', ja(prof), 'zones', jarr(zones), 'occupations', jarr(o),
                        'allow_visitors', jb(fset('VISITORS_ALLOWED')),
                        -- extras
                        'type', ja(L.type), 'name_native', js(try(dfhack.translation.translateName, ab.name, false) or ''),
                        'religion', ji(religion), 'deity_type', ji(dtype), 'flags', jarr(fl),
                        'non_citizens_allowed', jb(fset('NON_CITIZENS_ALLOWED')), 'members_only', jb(fset('MEMBERS_ONLY')),
                        'exists', jb(not fset('DOES_NOT_EXIST')), 'location_value', ji(L.value), 'location_tier', ji(L.tier),
                        'tier_name', ja(L.tier_name), 'supplies', jarr(need), 'occupations_detail', jarr(od) })
        end)
        if ok then out[#out + 1] = s else warn('location', s) end
      end
    end)
    write_file(OUT .. '/locations.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
    counts.locations, counts.occupations = #out, occs
    checkpoint()
  end

  local function hf_name(id)
    if not id or id < 0 then return '' end
    return try(function() return dfhack.translation.translateName(df.historical_figure.find(id).name, true) end) or ''
  end
  local function export_crimes()
    phase = 'crimes'
    local ids = {}
    for _, c in ipairs(df.global.world.crimes.all) do if c.site == SITE_ID then ids[#ids + 1] = c.id end end
    local out = {}
    for i = 1, #ids do
      local ok, s = pcall(function()
        local c = df.crime.find(ids[i])
        if not c then return nil end
        local sentenced = c.flags.sentenced
        local pun = {}
        if sentenced then
          if c.punishment.hammerstrikes > 0 then pun[#pun + 1] = 'Hammerstrikes ' .. c.punishment.hammerstrikes end
          if c.punishment.prison_time > 0 then pun[#pun + 1] = 'Jail ' .. c.punishment.prison_time end
          if c.punishment.flags.beating then pun[#pun + 1] = 'Beating' end
          if c.punishment.flags.exiled then pun[#pun + 1] = 'Exile' end
          if c.punishment.flags.death_sentence then pun[#pun + 1] = 'Death' end
        end
        local wit, seen = {}, {}
        for _, w in ipairs(c.witnesses) do
          if w.witness_id >= 0 and not seen[w.witness_id] then seen[w.witness_id] = true; wit[#wit + 1] = ji(w.witness_id) end
        end
        local chf = try(function() return c.criminal_hf.hfid end) or -1
        local vhf = try(function() return c.victim_hf.hfid end) or -1
        return jobj({ 'id', ji(c.id), 'kind', ja(df.crime_type[c.mode] or tostring(c.mode)), 'criminal', ji(c.criminal),
                      'victim', ji(c.victim), 'site_tick', ji(c.event_time), 'year', ji(c.event_year),
                      'convicted', ji(sentenced and c.accused or -1), 'punishment', ja(table.concat(pun, ', ')),
                      'witnesses', jarr(wit),
                      -- extras
                      'accused', ji(c.accused), 'criminal_hf', ji(chf), 'victim_hf', ji(vhf),
                      'criminal_name', js(hf_name(chf)), 'victim_name', js(hf_name(vhf)),
                      'flags', jarr(set_flags(c.flags, false)), 'hammerstrikes', ji(c.punishment.hammerstrikes),
                      'prison_time', ji(c.punishment.prison_time), 'punishment_flags', jarr(set_flags(c.punishment.flags, false)),
                      'discovered_year', ji(c.discovered_year), 'discovered_tick', ji(c.discovered_time),
                      'entity', ji(c.entity), 'item_id', ji(c.item_id), 'incident_id', ji(c.incident_id) })
      end)
      if ok and s then out[#out + 1] = s elseif not ok then warn('crime', s) end
      if i % 16 == 0 then phase_detail = i .. '/' .. #ids; checkpoint() end
    end
    write_file(OUT .. '/crimes.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
    counts.crimes = #out
    -- punishments being carried out now (plotinfo.punishments): rf_import::PunishmentInfo via load_extras
    local pun = {}
    pcall(function()
      for _, p in ipairs(df.global.plotinfo.punishments) do
        local vic = {}
        for _, v in ipairs(p.victims) do vic[#vic + 1] = ji(v) end
        pun[#pun + 1] = jobj({ 'criminal', ji(p.criminal), 'officer', ji(p.officer), 'beating', ji(p.beating),
                               'hammerstrikes', ji(p.hammer_strikes), 'prison_counter', ji(p.prison_counter),
                               'time_to_assign', ji(p.time_to_assign), 'chain', ji(p.chain), 'victims', jarr(vic) })
      end
    end)
    write_file(OUT .. '/punishments.json', '[\n' .. table.concat(pun, ',\n') .. '\n]\n')
    counts.punishments = #pun
  end

  local function export_haul_routes()
    phase = 'haul_routes'
    local out = {}
    pcall(function()
      for _, r in ipairs(df.global.plotinfo.hauling.routes) do
        local ok, s = pcall(function()
          local stops = {}
          for _, st in ipairs(r.stops) do
            local take, give, links = {}, {}, {}
            for _, l in ipairs(st.stockpiles) do
              if l.mode.take then take[#take + 1] = ji(l.building_id) end
              if l.mode.give then give[#give + 1] = ji(l.building_id) end
              links[#links + 1] = jobj({ 'building_id', ji(l.building_id), 'mode', jarr(set_flags(l.mode, false)) })
            end
            local conds = {}
            for _, c in ipairs(st.conditions) do
              local dir = try(function() return df.stop_depart_condition.T_direction[c.direction] end) or tostring(c.direction)
              local mode = try(function() return df.stop_depart_condition.T_mode[c.mode] end) or tostring(c.mode)
              local txt = mode .. ' ' .. dir
              if c.timeout > 0 then txt = txt .. ' after ' .. c.timeout .. ' ticks' end
              local cf = {}
              for _, f in ipairs(set_flags(c.flags, false)) do cf[#cf + 1] = f:gsub('"', '') end
              if c.load_percent > 0 then
                local cmp = '>='
                for _, f in ipairs(cf) do if f == 'at_most' then cmp = '<=' end end
                txt = txt .. ' when ' .. cmp .. ' ' .. c.load_percent .. '% full'
              end
              if #cf > 0 then txt = txt .. ' [' .. table.concat(cf, ',') .. ']' end
              conds[#conds + 1] = ja(txt)
            end
            stops[#stops + 1] = jobj({ 'name', js(st.name), 'pos', jpos(st.pos.x, st.pos.y, st.pos.z), 'take_from', jarr(take),
                                       'give_to', jarr(give), 'conditions', jarr(conds),
                                       -- extras
                                       'id', ji(st.id), 'cart_id', ji(st.cart_id), 'time_waiting', ji(st.time_waiting),
                                       'links', jarr(links) })
          end
          local veh = {}
          for _, v in ipairs(r.vehicle_ids) do veh[#veh + 1] = ji(v) end
          local vst = {}
          pcall(function() for _, v in ipairs(r.vehicle_stops) do vst[#vst + 1] = ji(v) end end)
          return jobj({ 'id', ji(r.id), 'name', js(r.name), 'stops', jarr(stops), 'vehicles', jarr(veh), 'vehicle_stops', jarr(vst) })
        end)
        if ok then out[#out + 1] = s else warn('haul_route', s) end
      end
    end)
    write_file(OUT .. '/haul_routes.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
    counts.haul_routes = #out
    checkpoint()
  end

  function r8.export_files()
    export_work_details()
    export_kitchen()
    export_locations()
    export_crimes()
    export_haul_routes()
  end
end

-- ------------------------------------------------------------------------------------------- engravings
-- round 5: engravings.json = rf_core::Engraving for every entry of world.event.engravings (DF 53.16:
-- the v50 home of the engraving list). pos is the engraved tile (the wall tile itself for wall
-- engravings; the side flags say which faces show it), floor = engraving_flags.floor, quality 0..5,
-- mat = palette index of the tile's material (same rules as map/zNNN.bin, added to the palette),
-- engraver = unit id of the artist's historical figure (-1 if dead or off the map), skill = skill_rating.
local export_engravings
do
  local vcache = {}   -- block key -> vein map
  local cpos          -- df.coord owned by this script (construction lookup)
  local function tile_mat(x, y, z)
    local blk = dfhack.maps.getTileBlock(x, y, z)
    if not blk then return 0xFFFF end
    local lx, ly = x & 15, y & 15
    local tt = blk.tiletype[lx][ly]
    local d = blk.designation[lx][ly].whole
    local cls = TT_MAT[tt]
    if cls == C_STONE or cls == C_SOIL or cls == C_POOL or cls == C_BROOK or cls == C_RIVER then
      return try(layer_mat, x, y, z, (d >> 10) & 15) or 0xFFFF
    elseif cls == C_MINERAL then
      local key = (z * 4096 + (y >> 4)) * 4096 + (x >> 4)
      local veins = vcache[key]
      if not veins then veins = vein_map(blk); vcache[key] = veins end
      return veins[lx * 16 + ly] or try(layer_mat, x, y, z, (d >> 10) & 15) or 0xFFFF
    elseif cls == C_CONS then
      cpos.x, cpos.y, cpos.z = x, y, z
      local con = df.construction.find(cpos)
      return con and palidx(con.mat_type, con.mat_index) or 0xFFFF
    elseif cls == C_FEATURE then
      return feature_mat(blk, d)
    elseif cls == C_LAVA or cls == C_MAGMA then
      return palfind('INORGANIC:OBSIDIAN')
    end
    return 0xFFFF
  end
  local HF_UNIT = {}
  local function engraver(hfid)
    if hfid == nil or hfid < 0 then return -1 end
    local u = HF_UNIT[hfid]
    if u == nil then u = (try(hf_unit, hfid)) or -1; HF_UNIT[hfid] = u end
    return u
  end
  function export_engravings()
    phase = 'engravings'
    cpos = df.coord:new()
    local out, seen = {}, {}
    local n_floor, n_wall, n_nomat, n_unit = 0, 0, 0, 0
    local i = 0
    while true do
      local list = df.global.world.event.engravings   -- re-fetched after every yield
      if i >= #list then break end
      local g = list[i]
      local ok, s = pcall(function()
        local x, y, z = g.pos.x, g.pos.y, g.pos.z
        local key = (z * 4096 + y) * 4096 + x
        if seen[key] then return nil end   -- the vector shifted between frames
        seen[key] = true
        local fl = g.flags.floor and true or false
        local mat = tile_mat(x, y, z)
        local uid = engraver(g.artist)
        local qv = math.max(0, math.min(5, g.quality or 0))
        if fl then n_floor = n_floor + 1 else n_wall = n_wall + 1 end
        if mat == 0xFFFF then n_nomat = n_nomat + 1 end
        if uid >= 0 then n_unit = n_unit + 1 end
        local sides = {}
        for _, k in ipairs({ 'north', 'south', 'east', 'west', 'northwest', 'northeast', 'southwest', 'southeast' }) do
          if flag(g.flags, k) then sides[#sides + 1] = ja(k) end
        end
        return jobj({ 'pos', jpos(x, y, z), 'quality', ji(qv), 'floor', jb(fl), 'mat', ji(mat),
                      'engraver', ji(uid), 'skill', ji(g.skill_rating or 0),
                      -- extras (not in rf_core::Engraving)
                      'artist_hf', ji(g.artist or -1), 'sides', jarr(sides), 'hidden', jb(flag(g.flags, 'hidden')),
                      'art_id', ji(g.art_id or -1), 'art_subid', ji(g.art_subid or -1),
                      'masterpiece_event', ji(g.masterpiece_event or -1) })
      end)
      if not ok then warn('engraving', s) elseif s then out[#out + 1] = s end
      i = i + 1
      if i % 32 == 0 then phase_detail = i .. '/' .. #list; checkpoint() end
    end
    write_file(OUT .. '/engravings.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
    counts.engravings = #out
    note(('round 5 engravings: %d (%d floor, %d wall), %d without resolvable material, %d engravers still on the map; pos = engraved tile (walls: the wall tile, sides = visible faces)')
         :format(#out, n_floor, n_wall, n_nomat, n_unit))
  end
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
-- round 9: squad ammunition (DF squad ammo screen), per-position uniforms, patrol route waypoints, supplies.
-- ammo item: SUBTYPE[@material token | @WOOD/BONE/METAL/<class>] (rf_core::SquadAmmo)
function r8.squad_r9(s, _, _)
  local out = { ammo = '[]', position_uniforms = '[]', patrol = '[]', route_id = -1, route_name = '', carry_food = -1,
                carry_water = -1, ammo_items = {} }
  local am = {}
  pcall(function()
    for _, a in ipairs(s.ammo.ammunition) do
      local item = subtype_token(a.item_type, a.item_subtype)
      if item == '' then item = item_type_name(a.item_type) end
      if a.mattype >= 0 then item = item .. '@' .. mat_token(a.mattype, a.matindex)
      elseif a.material_class >= 0 then
        local cl = tostring(df.entity_material_category[a.material_class] or a.material_class):upper()
        item = item .. '@' .. cl
      end
      am[#am + 1] = jobj({ 'item', js(item), 'amount', ji(a.amount), 'combat', jb(a.flags.use_combat),
                           'training', jb(a.flags.use_training) })
    end
    for _, iid in ipairs(s.ammo.ammo_items) do out.ammo_items[#out.ammo_items + 1] = ji(iid) end
  end)
  out.ammo = jarr(am)
  local pu = {}
  for _, pos in ipairs(s.positions) do
    local l = {}
    pcall(function()
      local uf = pos.equipment.uniform
      for cat = 0, df.uniform_category._last_item do
        for _, sp in ipairs(uf[cat]) do l[#l + 1] = js(uniform_spec(sp)) end
      end
    end)
    pu[#pu + 1] = jarr(l)
  end
  out.position_uniforms = jarr(pu)
  -- the patrol route of a PATROL_ROUTE order (explicit squad orders, else this month's schedule)
  pcall(function()
    local function find_route(orders, get)
      for _, o in ipairs(orders) do
        local oo = get(o)
        if df.squad_order_type[oo:getType()] == 'PATROL_ROUTE' then return oo.route_id end
      end
    end
    local rid = find_route(s.orders, function(o) return o end)
    if not rid then
      local month = df.global.cur_year_tick // 33600
      rid = find_route(s.schedule.routine[s.cur_routine_idx].month[month].orders, function(o) return o.order end)
    end
    if rid then
      out.route_id = rid
      local W = df.global.plotinfo.waypoints
      local pts = {}
      for _, p in ipairs(W.points) do pts[p.id] = p end
      for _, r in ipairs(W.routes) do
        if r.id == rid then
          out.route_name = r.name
          local wp = {}
          for _, pid in ipairs(r.points) do
            local p = pts[pid]
            if p then wp[#wp + 1] = jpos(p.pos.x, p.pos.y, p.pos.z) end
          end
          out.patrol = jarr(wp)
        end
      end
    end
  end)
  pcall(function() out.carry_food, out.carry_water = s.supplies.carry_food, s.supplies.carry_water end)
  return out
end
local function squad_json(s)
  local month = df.global.cur_year_tick // 33600
  local mem, hfs, uni_by_pos, pos_orders, seen_uni, uni = {}, {}, {}, {}, {}, {}
  for _, pos in ipairs(s.positions) do
    local uid = -1
    if pos.occupant >= 0 then
      local hf = df.historical_figure.find(pos.occupant)
      uid = hf and hf.unit_id or -1
      hf_ref(pos.occupant)
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
  local r9 = r8.squad_r9(s, uni_by_pos, current)   -- round 9: ammo, per-position uniforms, patrol waypoints
  local tname = try(dfhack.translation.translateName, s.name, true) or ''
  return jobj({ 'id', ji(s.id), 'name', js(tname), 'alias', js(s.alias), 'members', jarr(mem), 'orders', jarr(current),
                'uniform', jarr(uni), 'schedule', js(routine_name),
                -- extras
                'display_name', js(s.alias ~= '' and s.alias or tname),
                'name_native', js(try(dfhack.translation.translateName, s.name, false) or ''),
                'member_hfs', jarr(hfs), 'positions', ji(#s.positions), 'explicit_orders', jarr(explicit),
                'schedule_orders', jarr(sched), 'schedule_month', sched_entry, 'position_orders', jarr(pos_orders),
                'uniform_by_position', jarr(uni_by_pos), 'routine_id', ji(s.cur_routine_idx),
                'leader_position', ji(s.leader_position), 'barracks', jarr(rooms),
                -- round 9: rf_core::Squad ammo / position_uniforms / patrol (+ extras)
                'ammo', r9.ammo, 'position_uniforms', r9.position_uniforms, 'patrol', r9.patrol,
                'patrol_route', ji(r9.route_id), 'patrol_route_name', js(r9.route_name),
                'carry_food', ji(r9.carry_food), 'carry_water', ji(r9.carry_water),
                'ammo_items', jarr(r9.ammo_items) })
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
    -- round 8: animal products. SHEARABLE is a Rustfort pseudo-flag (DF: [SHEARABLE_TISSUE_LAYER] entries)
    local cflags = set_flags(c.flags, false)
    local shear = {}
    pcall(function()
      for _, sh in ipairs(c.shearable_tissue_layer) do
        local parts = {}
        for k = 0, #sh.part_idx - 1 do
          parts[#parts + 1] = ja(try(function() return bi.body_parts[sh.part_idx[k]].token end) or tostring(sh.part_idx[k]))
        end
        shear[#shear + 1] = jobj({ 'length', ji(sh.length), 'parts', jarr(parts) })
      end
    end)
    if #shear > 0 then cflags[#cflags + 1] = ja('SHEARABLE') end
    local milk = try(function()
      if c.extracts.milkable_mat < 0 then return nil end
      return jobj({ 'mat', js(mat_token(c.extracts.milkable_mat, c.extracts.milkable_matidx)), 'frequency', ji(c.misc.milkable) })
    end) or 'null'
    local eggs = {}
    pcall(function()
      for k = 0, #c.extracts.egg_material_mattype - 1 do
        eggs[#eggs + 1] = js(mat_token(c.extracts.egg_material_mattype[k], c.extracts.egg_material_matindex[k]))
      end
    end)
    castes[#castes + 1] = jobj({ 'token', ja(c.caste_id), 'name', js(c.caste_name[0]), 'body_size', ji(adult),
                                 'walk_speed', ji(caste_walk_speed(ri, ci, c) or 900), 'attacks', jarr(attacks),
                                 'body_parts', jarr(pj), 'flags', jarr(cflags),
                                 -- round-8 extras: animal products and husbandry numbers from the caste raws
                                 'milkable', milk, 'grazer', ji(flag(c.flags, 'GRAZER') and (try(function() return c.misc.grazer end) or 0) or -1),
                                 'shearable', jarr(shear), 'egg_materials', jarr(eggs),
                                 'clutch_size', jarr({ ji(try(function() return c.misc.clutch_size_min end) or 0),
                                                       ji(try(function() return c.misc.clutch_size_max end) or 0) }),
                                 'egg_size', ji(try(function() return c.misc.egg_size end) or 0),
                                 'litter_size', jarr({ ji(try(function() return c.misc.litter_size_min end) or 0),
                                                       ji(try(function() return c.misc.litter_size_max end) or 0) }),
                                 'pet_value', ji(try(function() return c.misc.petvalue end) or 0),
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

-- ------------------------------------------------------------------------------------------- society (round 4)
-- entities.json, nobles.json, settings.json, hist_figs.json, caravans.json, diplomats.json
local soc = { entities = 0, nobles = 0, mandates = 0, hist_figs = 0, caravans = 0, diplomats = 0, merchant_events = 0 }
local export_society   -- defined in the block below (helpers block-local: 200-locals limit)
do
  local function race_token(ri)
    if ri == nil or ri < 0 then return '' end
    return try(function() return raws.creatures.all[ri].creature_id end) or ''
  end
  local function tname(nm, eng) return try(dfhack.translation.translateName, nm, eng) or '' end
  local function jil(l) local o = {} for i = 1, #l do o[i] = ji(l[i]) end return jarr(o) end
  local function vec_ids(v) local o = {} pcall(function() for _, x in ipairs(v) do o[#o + 1] = x end end) return o end

  -- merchant history events at our site: which entities have sent caravans here (DF: history_event_merchantst)
  local MERCHANT_SOURCES = {}   -- [entity id] = { n, first_year, last_year }
  local function scan_merchant_events()
    phase = 'history'
    local T = df.history_event_type.MERCHANT
    local i = 0
    while true do
      local ev = df.global.world.history.events   -- re-fetched after every yield; the vector only grows
      if i >= #ev then break end
      pcall(function()
        local e = ev[i]
        if e:getType() == T and e.site == SITE_ID then
          local m = MERCHANT_SOURCES[e.source]
          if not m then m = { 0, e.year, e.year }; MERCHANT_SOURCES[e.source] = m end
          m[1] = m[1] + 1
          if e.year < m[2] then m[2] = e.year end
          if e.year > m[3] then m[3] = e.year end
          soc.merchant_events = soc.merchant_events + 1
        end
      end)
      i = i + 1
      if i % 1000 == 0 then phase_detail = i .. '/' .. #ev; checkpoint() end
    end
  end

  local function diplomacy_of(e)
    local out = {}
    pcall(function()
      for _, s in ipairs(e.relations.diplomacy.state) do
        out[s.group_id] = { relation = s.relation, flags = set_flags(s.flags, true), war = s.war_event_collection }
      end
    end)
    return out
  end
  local function export_entities()
    phase = 'entities'
    local pi = df.global.plotinfo
    local civ_id, grp_id = pi.civ_id, pi.group_id
    local civ = df.historical_entity.find(civ_id)
    local grp = df.historical_entity.find(grp_id)
    local dip_civ = civ and diplomacy_of(civ) or {}
    local dip_grp = grp and diplomacy_of(grp) or {}
    local why, ids = {}, {}
    local function want(id, reason)
      if id == nil or id < 0 then return end
      if not why[id] then why[id] = {}; ids[#ids + 1] = id end
      why[id][reason] = true
    end
    want(civ_id, 'own'); want(grp_id, 'own')
    for id in pairs(dip_civ) do want(id, 'diplomacy') end
    for id in pairs(dip_grp) do want(id, 'diplomacy') end
    pcall(function() for _, c in ipairs(pi.caravans) do want(c.entity, 'caravan') end end)
    pcall(function() for _, d in ipairs(pi.dip_meeting_info) do want(d.civ_id, 'diplomat') end end)
    for id in pairs(MERCHANT_SOURCES) do want(id, 'trade') end
    for id in pairs(CIV_SEEN) do want(id, 'units') end
    pcall(function()
      for _, e in ipairs(df.global.world.entities.all) do
        if e.type == df.historical_entity_type.Civilization then want(e.id, 'civilization') end
      end
    end)
    table.sort(ids)
    local caravan_of = {}
    pcall(function() for _, c in ipairs(pi.caravans) do caravan_of[c.entity] = true end end)
    local out = {}
    for i = 1, #ids do
      local id = ids[i]
      local ok, s = pcall(function()
        local e = df.historical_entity.find(id)
        if not e then return nil end
        local relation, d = 'Unknown', dip_civ[id] or dip_grp[id]
        if id == civ_id or id == grp_id then relation = 'Own'
        elseif d then relation = (d.relation == 1) and 'War' or 'Peace' end
        local parent = -1
        pcall(function()
          for _, l in ipairs(e.entity_links) do
            if l.type == df.entity_entity_link_type.PARENT then parent = l.target end
          end
        end)
        local deities = vec_ids(e.relations.deities)
        for _, h in ipairs(deities) do hf_ref(h) end
        local reasons = {}
        for r in pairs(why[id]) do reasons[#reasons + 1] = r end
        table.sort(reasons)
        local rj = {}
        for _, r in ipairs(reasons) do rj[#rj + 1] = ja(r) end
        local m = MERCHANT_SOURCES[id]
        return jobj({ 'id', ji(id), 'name', js(tname(e.name, true)), 'race', ja(race_token(e.race)),
                      'kind', ja(df.historical_entity_type[e.type] or tostring(e.type)), 'relation', ja(relation),
                      'trades', jb(m ~= nil or caravan_of[id] == true),
                      -- extras
                      'name_native', js(tname(e.name, false)), 'parent', ji(parent), 'reasons', jarr(rj),
                      'diplomacy_flags', jarr(d and d.flags or {}), 'war_event_collection', ji(d and d.war or -1),
                      'caravans_sent', ji(m and m[1] or 0), 'first_caravan_year', ji(m and m[2] or -1),
                      'last_caravan_year', ji(m and m[3] or -1), 'units_on_map', ji(CIV_SEEN[id] or 0),
                      'hostility_level', ji(try(function() return e.hostility_level end) or 0),
                      'siege_tier', ji(try(function() return e.siege_tier end) or 0), 'deities', jil(deities) })
      end)
      if ok and s then out[#out + 1] = s elseif not ok then warn('entity', s) end
      if i % 8 == 0 then checkpoint() end
    end
    write_file(OUT .. '/entities.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
    soc.entities = #out
  end

  -- mandates as DF-like text: "Make 1 spear", "Make 2 crowns (gold)", "Ban export of steel bars"
  local function mandate_text(m)
    local mode = df.mandate_type[m.mode] or tostring(m.mode)
    local def = try(dfhack.items.getSubtypeDef, m.item_type, m.item_subtype)
    local one = def and def.name or (df.item_type[m.item_type] or 'item'):lower():gsub('_', ' ')
    local many = def and (try(function() return def.name_plural end) or one) or (one .. 's')
    local mat = ''
    if m.mat_type >= 0 then
      local mi = try(dfhack.matinfo.decode, m.mat_type, m.mat_index)
      mat = mi and (try(function() return mi:toString() end) or '') or ''
    end
    if mode == 'Export' then return 'Ban export of ' .. (mat ~= '' and (mat .. ' ') or '') .. many end
    local noun = (m.amount_total == 1) and one or many
    local s = (mode == 'Make' and 'Make ' or (mode .. ': ')) .. m.amount_total .. ' ' .. noun
    if mat ~= '' then s = s .. ' (' .. mat .. ')' end
    return s
  end
  local function mandate_detail(m)
    return jobj({ 'text', js(mandate_text(m)), 'mode', ja(df.mandate_type[m.mode] or tostring(m.mode)),
                  'item_type', ja(item_type_name(m.item_type)), 'item_subtype', js(subtype_token(m.item_type, m.item_subtype)),
                  'mat', js(mat_token(m.mat_type, m.mat_index)), 'amount_total', ji(m.amount_total),
                  'amount_remaining', ji(m.amount_remaining), 'timeout_counter', ji(m.timeout_counter),
                  'timeout_limit', ji(m.timeout_limit),
                  'punishment_hammerstrikes', ji(try(function() return m.punishment.hammerstrikes end) or 0),
                  'punishment_prison_time', ji(try(function() return m.punishment.prison_time end) or 0) })
  end
  local function export_nobles()
    phase = 'nobles'
    local pi = df.global.plotinfo
    -- mandates per issuing unit
    local mand = {}
    pcall(function()
      for _, m in ipairs(df.global.world.mandates.all) do
        local uid = m.unit and m.unit.id or -1
        mand[uid] = mand[uid] or {}
        table.insert(mand[uid], m)
      end
    end)
    local attached = {}
    local out = {}
    for _, eid in ipairs({ pi.group_id, pi.civ_id }) do
      local e = df.historical_entity.find(eid)
      if e then
        local pos_by_id = {}
        pcall(function() for _, p in ipairs(e.positions.own) do pos_by_id[p.id] = p end end)
        -- the first assignment of a holder whose position can issue mandates (mandate_max > 0) gets the mandates
        local rows = {}
        for _, a in ipairs(e.positions.assignments) do
          local ok, row = pcall(function()
            local p = pos_by_id[a.position_id]
            local hfid = a.histfig
            hf_ref(hfid)
            local uid, uid_any = -1, -1
            local hf = hfid >= 0 and df.historical_figure.find(hfid) or nil
            if hf then uid, uid_any = hf_unit(hfid) end
            local name = p and p.name[0] or ''
            pcall(function()
              if hf and hf.sex == 0 and p.name_female[0] ~= '' then name = p.name_female[0] end
              if hf and hf.sex == 1 and p.name_male[0] ~= '' then name = p.name_male[0] end
            end)
            local resp = {}
            pcall(function()
              for k, v in pairs(p.responsibilities) do if v == true then resp[#resp + 1] = ja(tostring(k)) end end
            end)
            table.sort(resp)
            return { a = a, p = p, uid = uid, uid_any = uid_any, hfid = hfid, hf = hf, name = name, resp = resp,
                     can_mandate = p and (try(function() return p.mandate_max end) or 0) > 0 }
          end)
          if ok then rows[#rows + 1] = row else warn('noble', row) end
        end
        for _, r in ipairs(rows) do
          local texts, details = {}, {}
          if r.uid_any >= 0 and mand[r.uid_any] and not attached[r.uid_any] and r.can_mandate then
            attached[r.uid_any] = true
            for _, m in ipairs(mand[r.uid_any]) do
              texts[#texts + 1] = js(mandate_text(m))
              details[#details + 1] = mandate_detail(m)
              soc.mandates = soc.mandates + 1
            end
          end
          local p, a = r.p, r.a
          local function pv(f) return ji(try(function() return p[f] end) or 0) end
          out[#out + 1] = jobj({ 'token', ja(p and p.code or ''), 'name', js(r.name), 'holder', ji(r.uid), 'entity_id', ji(eid),
            'mandates', jarr(texts),
            -- extras
            'holder_hf', ji(r.hfid), 'holder_unit_any', ji(r.uid_any),
            'holder_name', js(r.hf and tname(r.hf.name, true) or ''), 'assignment_id', ji(a.id), 'position_id', ji(a.position_id),
            'squad_id', ji(try(function() return a.squad_id end) or -1), 'responsibilities', jarr(r.resp),
            'flags', jarr(p and set_flags(p.flags, true) or {}), 'precedence', pv('precedence'), 'number', pv('number'),
            'mandate_max', pv('mandate_max'), 'demand_max', pv('demand_max'),
            'required_bedroom', pv('required_bedroom'), 'required_office', pv('required_office'),
            'required_dining', pv('required_dining'), 'required_tomb', pv('required_tomb'),
            'required_boxes', pv('required_boxes'), 'required_cabinets', pv('required_cabinets'),
            'required_racks', pv('required_racks'), 'required_stands', pv('required_stands'),
            'mandates_detail', jarr(details) })
        end
      end
      checkpoint()
    end
    -- mandates of units that hold no mandate position any more (DF keeps them running)
    for uid, list in pairs(mand) do
      if not attached[uid] then note(('nobles: %d mandate(s) of unit %d not attached to a position'):format(#list, uid)) end
    end
    write_file(OUT .. '/nobles.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
    soc.nobles = #out
  end

  -- settings.json: d_init (population caps, world features) + the save's difficulty settings (DF 50+)
  local function export_settings()
    phase = 'settings'
    local other = {}
    local function kv(k, v) other[#other + 1] = js(k .. '=' .. tostring(v)) end
    local dw = df.global.d_init.dwarf
    local ff = df.global.d_init.feature.flags
    local fe = ff._enum or df.d_init_flags4
    local function feat(name) return try(function() return ff[fe[name]] end) and true or false end
    local cd = df.global.plotinfo.main.custom_difficulty
    local enemies = enum_name(cd, 'difficulty_enemies')
    local sieges_flag = try(function() return cd.flags.sieges end) and true or false
    -- difficulty "Enemies: Off" disables invasions; "Custom" uses the sieges flag; Normal/Hard have sieges
    local invasions = enemies ~= 'Off' and (enemies ~= 'Custom' or sieges_flag)
    kv('DIFFICULTY_ENEMIES', enemies)
    kv('DIFFICULTY_ECONOMY', enum_name(cd, 'difficulty_economy'))
    for _, k in ipairs({ 'visitor_cap', 'baby_cap_percent', 'specific_seed_cap', 'fortress_seed_cap', 'graze_coefficient',
                         'cull_dead_units_at', 'maximum_embark_dim' }) do
      pcall(function() kv(k:upper(), dw[k]) end)
    end
    pcall(function()
      for i = 0, #ff - 1 do
        local nm = fe[i]
        if nm and not tostring(nm):match('^UNUSED') then kv('FEATURE_' .. nm, ff[i] and 'true' or 'false') end
      end
    end)
    -- every difficulty field: scalars as KEY=value, arrays as KEY=a,b,c, flags as KEY=true/false
    local keys = {}
    pcall(function() for k in pairs(cd) do keys[#keys + 1] = k end end)
    table.sort(keys)
    for _, k in ipairs(keys) do
      pcall(function()
        local v = cd[k]
        if k == 'difficulty_enemies' or k == 'difficulty_economy' then return end   -- written above by name
        if type(v) == 'number' then kv(k:upper(), v)
        elseif k == 'flags' then
          for i = 0, 31 do local nm = v._type[i]; if nm then kv('FLAG_' .. nm:upper(), v[nm] and 'true' or 'false') end end
        elseif type(v) == 'userdata' then
          local a = {}
          for j = 0, #v - 1 do a[#a + 1] = tostring(v[j]) end   -- static arrays: indexing past the end raises
          kv(k:upper(), table.concat(a, ','))
        end
      end)
    end
    pcall(function()
      local m = df.global.plotinfo.main
      for _, k in ipairs({ 'traffic_cost_high', 'traffic_cost_normal', 'traffic_cost_low', 'traffic_cost_restricted' }) do kv(k:upper(), m[k]) end
    end)
    local wealth9 = r8.settings_r9(kv)   -- round 9: DF's wealth statistics, invasion tiers, fort statistics
    local s = jobj({ 'population_cap', ji(dw.population_cap), 'strict_population_cap', ji(dw.strict_population_cap),
                     'baby_cap', ji(dw.baby_cap_absolute), 'invasions', jb(invasions), 'temperature', jb(feat('TEMPERATURE')),
                     'weather', jb(feat('WEATHER')), 'cave_ins', jb(feat('CAVEINS')), 'other', jarr(other),
                     -- extras
                     'baby_cap_percent', ji(dw.baby_cap_percent), 'visitor_cap', ji(dw.visitor_cap),
                     'artifacts', jb(feat('ARTIFACTS')), 'difficulty_enemies', ja(enemies),
                     'wealth', wealth9,
                     'source', ja('d_init.dwarf, d_init.feature.flags, plotinfo.main.custom_difficulty, plotinfo.tasks.wealth, plotinfo.progress_*, plotinfo.invasions') })
    write_file(OUT .. '/settings.json', s .. '\n')
    checkpoint()
  end

  -- caravans.json (rf_core::Caravan) and diplomats.json
  local function export_trade()
    phase = 'trade'
    local pi = df.global.plotinfo
    local depots = {}
    pcall(function()
      for _, b in ipairs(df.global.world.buildings.all) do
        if b:getType() == BT.TradeDepot then
          depots[#depots + 1] = { id = b.id, acc = try(function() return b.accessible end) and true or false }
        end
      end
    end)
    table.sort(depots, function(a, b) if a.acc ~= b.acc then return a.acc end return a.id < b.id end)
    local depot = depots[1] and depots[1].id or -1
    local merchants, diplomat_units = {}, {}
    pcall(function()
      for _, u in ipairs(df.global.world.units.active) do
        if q(dfhack.units.isMerchant, u) then
          merchants[u.civ_id] = merchants[u.civ_id] or {}
          table.insert(merchants[u.civ_id], u.id)
        end
        if q(dfhack.units.isDiplomat, u) then diplomat_units[#diplomat_units + 1] = u end
      end
    end)
    local cars = {}
    pcall(function()
      for _, c in ipairs(pi.caravans) do
        local ok, s = pcall(function()
          local e = df.historical_entity.find(c.entity)
          return jobj({ 'entity_id', ji(c.entity), 'civ', js(e and tname(e.name, true) or ''), 'race', ja(e and race_token(e.race) or ''),
            'state', ja(enum_name(c, 'trade_state')), 'leaves_in', ji(c.time_remaining * 10), 'depot', ji(depot),
            'goods', jil(vec_ids(c.goods)), 'animals', jil(vec_ids(c.animals)), 'merchants', jil(merchants[c.entity] or {}),
            'import_value', ji(c.import_value), 'export_value', ji(c.export_value_total), 'mood', ji(c.mood),
            'flags', jarr(set_flags(c.flags, true)),
            -- extras
            'time_remaining', ji(c.time_remaining), 'depot_notified', ji(c.depot_notified),
            'export_value_personal', ji(c.export_value_personal), 'offer_value', ji(c.offer_value),
            'haggle_fail_count', ji(c.haggle_fail_count), 'civ_native', js(e and tname(e.name, false) or '') })
        end)
        if ok then cars[#cars + 1] = s else warn('caravan', s) end
      end
    end)
    write_file(OUT .. '/caravans.json', '[\n' .. table.concat(cars, ',\n') .. '\n]\n')
    soc.caravans = #cars
    local dips, seen = {}, {}
    pcall(function()
      for _, d in ipairs(pi.dip_meeting_info) do
        hf_ref(d.diplomat_id); hf_ref(d.associate_id)
        local uid = hf_unit(d.diplomat_id)
        if uid >= 0 then seen[uid] = true end
        dips[#dips + 1] = jobj({ 'civ_id', ji(d.civ_id), 'diplomat_hf', ji(d.diplomat_id), 'diplomat_unit', ji(uid),
          'associate_hf', ji(d.associate_id), 'associate_unit', ji(d.associate_id >= 0 and hf_unit(d.associate_id) or -1),
          'flags', jarr(set_flags(d.flags, true)), 'source', ja('meeting') })
      end
    end)
    for _, u in ipairs(diplomat_units) do
      if not seen[u.id] then
        hf_ref(u.hist_figure_id)
        dips[#dips + 1] = jobj({ 'civ_id', ji(u.civ_id), 'diplomat_hf', ji(u.hist_figure_id), 'diplomat_unit', ji(u.id),
          'associate_hf', '-1', 'associate_unit', '-1', 'flags', '[]', 'source', ja('unit') })
      end
    end
    write_file(OUT .. '/diplomats.json', '[\n' .. table.concat(dips, ',\n') .. '\n]\n')
    soc.diplomats = #dips
    counts.trade_depots = #depots
    checkpoint()
  end

  -- hist_figs.json: rf_core::HistFigName for every referenced historical figure (written last: collects the refs)
  local function export_hist_figs()
    phase = 'hist_figs'
    -- the civ's and the fort government's deities
    pcall(function()
      for _, eid in ipairs({ df.global.plotinfo.civ_id, df.global.plotinfo.group_id }) do
        local e = df.historical_entity.find(eid)
        for _, h in ipairs(e.relations.deities) do hf_ref(h) end
        for _, h in ipairs(e.relations.worship) do hf_ref(h) end
      end
    end)
    local ids = {}
    for id in pairs(HF_REF) do ids[#ids + 1] = id end
    table.sort(ids)
    local out = {}
    for i = 1, #ids do
      local ok, s = pcall(function()
        local h = df.historical_figure.find(ids[i])
        if not h then return nil end
        local uid, uid_any, dead = hf_unit(h.id)
        local name = tname(h.name, true)
        local race = race_token(h.race)
        if name == '' then name = try(function() return raws.creatures.all[h.race].name[0] end) or race:lower() end
        local spheres = {}
        pcall(function()
          for _, sp in ipairs(h.info.metaphysical.spheres) do spheres[#spheres + 1] = ja(df.sphere_type[sp] or tostring(sp)) end
        end)
        if #spheres == 0 then
          pcall(function()
            for _, sp in ipairs(h.info.spheres.spheres) do spheres[#spheres + 1] = ja(df.sphere_type[sp] or tostring(sp)) end
          end)
        end
        return jobj({ 'id', ji(h.id), 'name', js(name), 'race', ja(race), 'alive', jb(not dead), 'unit_id', ji(uid),
                      -- extras
                      'name_native', js(tname(h.name, false)), 'unit_id_any', ji(uid_any), 'sex', ji(h.sex),
                      'born_year', ji(h.born_year), 'died_year', ji(h.died_year), 'civ_id', ji(h.civ_id),
                      'deity', jb(try(function() return h.flags.deity end) or false),
                      'profession', ja(df.profession[h.profession] or ''), 'spheres', jarr(spheres) })
      end)
      if ok and s then out[#out + 1] = s elseif not ok then warn('hist_fig', s) end
      if i % 32 == 0 then phase_detail = i .. '/' .. #ids; checkpoint() end
    end
    write_file(OUT .. '/hist_figs.json', '[\n' .. table.concat(out, ',\n') .. '\n]\n')
    soc.hist_figs = #out
  end

  function export_society()
    scan_merchant_events()
    export_entities()
    export_nobles()
    export_settings()
    export_trade()
    export_hist_figs()      -- last: entities/nobles/diplomats add references
  end
end

-- ------------------------------------------------------------------------------------------- round 9
-- DF's wealth statistics (plotinfo.tasks.wealth: created = total, imported/exported are trade totals), the
-- invasion tiers DF reached (plotinfo.progress_population/trade/production, compared with the ENEMY_*_TRIGGER
-- difficulty lists), fort statistics, and the invasion history (plotinfo.invasions.list -> invasions.json).
-- Called from the settings phase; kv(key, value) adds a settings.other entry. Returns the 'wealth' extras object.
function r8.settings_r9(kv)
  local P = df.global.plotinfo
  local obj = {}
  local function put(key, field, v)
    if v == nil then return end
    if type(v) == 'boolean' then v = v and 'true' or 'false' end
    kv(key, v)
    obj[#obj + 1] = field
    obj[#obj + 1] = (v == 'true' or v == 'false') and v or ji(tonumber(v) or 0)
  end
  local W = try(function() return P.tasks.wealth end)
  if W then
    for _, p in ipairs({ { 'WEALTH_CREATED', 'total' }, { 'WEALTH_EXPORTED', 'exported' }, { 'WEALTH_IMPORTED', 'imported' },
                         { 'WEALTH_OFFERED', 'offered' }, { 'WEALTH_WEAPONS', 'weapons' }, { 'WEALTH_ARMOR', 'armor' },
                         { 'WEALTH_FURNITURE', 'furniture' }, { 'WEALTH_OTHER', 'other' }, { 'WEALTH_ARCHITECTURE', 'architecture' },
                         { 'WEALTH_DISPLAYED', 'displayed' }, { 'WEALTH_HELD', 'held' } }) do
      put(p[1], p[2] == 'total' and 'created' or p[2], try(function() return W[p[2]] end))
    end
  end
  for _, p in ipairs({ 'progress_population', 'progress_trade', 'progress_production', 'fortress_rank', 'fortress_age',
                       'trees_removed', 'game_over', 'lost_to_siege_civ' }) do
    put(p:upper(), p, try(function() return P[p] end))
  end
  for _, p in ipairs({ 'total_deaths', 'total_insanities', 'total_executions', 'num_artifacts', 'invaders_repelled',
                       'excavated_tiles' }) do
    put('FORT_' .. p:upper(), p, try(function() return P.tasks[p] end))
  end
  -- invasion history
  local inv, hist = {}, {}
  pcall(function()
    for _, iv in ipairs(P.invasions.list) do
      local fl = iv.flags
      inv[#inv + 1] = jobj({ 'id', ji(iv.id), 'civ_id', ji(iv.civ_id), 'size', ji(iv.size), 'year', ji(iv.created_year),
                             'year_tick', ji(iv.created_season_count), 'siege', jb(fl.siege), 'active', jb(fl.active),
                             'mission', ji(iv.mission), 'active_size1', ji(iv.active_size1), 'active_size2', ji(iv.active_size2),
                             'duration_counter', ji(iv.duration_counter), 'total_death_cage_number', ji(iv.total_death_cage_number),
                             'layer_source', jb(fl.layer_source), 'undead_source', jb(fl.undead_source) })
      hist[#hist + 1] = ('%d:%d:%d:%s'):format(iv.created_year, iv.created_season_count, iv.size, fl.siege and 'siege' or 'raid')
    end
  end)
  kv('INVASION_HISTORY', table.concat(hist, ','))
  write_file(OUT .. '/invasions.json', '[\n' .. table.concat(inv, ',\n') .. '\n]\n')
  counts.invasions = #inv
  return jobj(obj)
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
  r8.prepare_locations()   -- round 8: locations and their civzones (for the room evaluation of civzones)
  r8.export_pile_filters() -- round 8: detailed stockpile filters, before the buildings phase writes them
  export_buildings()
  export_engravings()      -- round 5: after the map, before the materials (adds tile materials to the palette)
  export_work_orders()
  export_plant_raws()      -- before the materials: adds the plants' STRUCTURAL/DRINK/SEED materials to the palette
  export_burrows_squads()
  export_creature_raws()   -- after units/items (races seen), before the materials: adds the tissue materials
  export_item_raws()
  r8.export_files()        -- round 8: work details, kitchen, locations, crimes, haul routes (before hist figs: deity refs)
  r8.export_pile_options() -- round 9: every stockpile leaf DF offers (stockpile_options.json)
  export_society()         -- round 4: entities, nobles, settings, caravans, diplomats, hist figs (last: collects refs)
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
  note(('round 4 buildings: %d doors (%d locked = door_flags.forbidden), %d bridges (%d raised), %d traps (%d levers, %d loaded), %d mechanism links, %d caged units')
       :format(r4.doors, r4.locked_doors, r4.bridges, r4.raised_bridges, r4.traps, r4.levers, r4.loaded_traps, r4.links, r4.caged_units))
  note(('round 4 society: %d entities, %d noble assignments, %d mandates, %d hist figs, %d caravans, %d diplomats, %d merchant events at this site')
       :format(soc.entities, soc.nobles, soc.mandates, soc.hist_figs, soc.caravans, soc.diplomats, soc.merchant_events))
  note(('round 8: %d stockpiles with %d filter strings, %d work detail memberships, %d pets with owner, %d pastured, %d marked for slaughter, %d pregnant, %d trained animals')
       :format(r8.piles, r8.pile_leaves, r8.wd_members, r8.pets, r8.pastured, r8.slaughter, r8.pregnant, r8.trained))
  for k, v in pairs(r8.list_mismatch) do note(('stockpile filters: %s has %d entries in DF, not the expected list length; tokens are #<index>'):format(k, v)) end
  note(('round 9 units: %d not simulated by DF and off the map (caravan animals that left etc., flags.inactive), %d caged units, %d placed at their cage item, %d chained')
       :format(r8.r9.offsite, r8.r9.caged, r8.r9.cage_pos, r8.r9.chained))
  note(('round 9: %d stockpile option leaves (inclusion rules as the DFHack stockpiles plugin, estimate); %d invasions in plotinfo.invasions; wealth from plotinfo.tasks.wealth (created = total)')
       :format(counts.stockpile_options or 0, counts.invasions or 0))
  note('room evaluation: DF 53.16 keeps no value for personal rooms (getRoomDescription is empty for v50 zones); room_value = location_value for location zones, else -1; room_thought = owner latest quality thought')
  note('caravans: leaves_in = time_remaining x 10 ticks (DFHack caravan.lua: 120 per day); trades = sent a caravan here (history MERCHANT events) or has one now')
end

local function finish()
  local T1 = now()
  local wd = df.global.world.world_data
  local mpz = {}
  pcall(function() mpz = { df.global.world.map.region_x, df.global.world.map.region_y, df.global.world.map.region_z } end)
  local cnt = {}
  counts.unit_needs, counts.unit_emotions, counts.unit_values = r3.needs, r3.emotions, r3.values
  counts.unit_relationships, counts.unit_wounds = r3.relationships, r3.wounds
  counts.entities, counts.nobles, counts.mandates, counts.hist_figs = soc.entities, soc.nobles, soc.mandates, soc.hist_figs
  counts.caravans, counts.diplomats, counts.building_links = soc.caravans, soc.diplomats, r4.links
  counts.levers, counts.locked_doors, counts.traps = r4.levers, r4.locked_doors, r4.traps
  counts.stockpile_filters, counts.pets, counts.pastured = r8.pile_leaves, r8.pets, r8.pastured
  counts.slaughter, counts.pregnant = r8.slaughter, r8.pregnant
  if r8.r9 then counts.offsite_units, counts.caged_units, counts.chained_units = r8.r9.offsite, r8.r9.caged, r8.r9.chained end
  for _, k in ipairs({ 'units', 'citizens', 'items', 'buildings', 'plants', 'materials', 'tiletypes', 'tiles', 'z_exported',
                       'work_orders', 'plant_raws', 'building_jobs', 'farm_plots', 'burrows', 'squads',
                       'burrow_tiles', 'creature_raws', 'weapon_raws', 'armor_raws', 'unit_needs', 'unit_emotions',
                       'unit_values', 'unit_relationships', 'unit_wounds', 'entities', 'nobles', 'mandates', 'hist_figs',
                       'caravans', 'diplomats', 'building_links', 'levers', 'locked_doors', 'traps', 'trade_depots',
                       'engravings', 'work_details', 'kitchen', 'locations', 'occupations', 'crimes', 'punishments',
                       'haul_routes', 'stockpile_filters', 'pets', 'pastured', 'slaughter', 'pregnant',
                       'stockpile_options', 'invasions', 'offsite_units', 'caged_units', 'chained_units' }) do
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
-- One readable line for people running this by hand, then the JSON line Rustfort reads.
print(('rustfort-export: exporting %s (%s) into %s - manifest.json appears there when done'):format(utf(FORT_EN), utf(FORT_NATIVE), OUT))
print(('{"ok":true,"started":true,"fort":%s,"fort_native":%s,"out":%s,"budget_ms":%d}'):format(
  js(FORT_EN), js(FORT_NATIVE), ja(OUT), opt.budget))
