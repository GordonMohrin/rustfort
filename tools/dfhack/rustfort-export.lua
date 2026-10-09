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

local VERSION = 1
local SCRIPT_REV = '2026-10-09a'

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
local function checkpoint()
  if now() >= deadline then
    progress(false)
    coroutine.yield()
    if cancelled then error('cancelled by user (cancel file)') end
  end
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
  return jobj({ 'mat_type', ji(mt), 'mat_index', ji(mi), 'token', js(token), 'name', js(name),
                'rgb', jarr({ ji(rgb[1]), ji(rgb[2]), ji(rgb[3]) }),
                'is_stone', jb(st), 'is_gem', jb(gem), 'is_metal', jb(metal), 'is_ore', jb(ore), 'is_soil', jb(soil),
                'is_wood', jb(wood), 'is_glass', jb(glass), 'tags', jarr(tags) })
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
                  trees = trees or tree_candidates(bx, by)
                  mat = nearest_tree(trees, x0 + lx, y, z)
                  if not mat then mat = 0xFFFF; unresolved.tree = (unresolved.tree or 0) + 1 end
                elseif cls == C_PLANT then
                  mat = plant_at[(z * YC + y) * XC + x0 + lx]
                  if not mat then mat = 0xFFFF; unresolved.plant = (unresolved.plant or 0) + 1 end
                elseif cls == C_FROZEN then
                  mat = WATER
                elseif cls == C_LAVA then
                  mat = OBSIDIAN
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
                local liq = (d & 7) | (((d >> 21) & 1) << 3)
                recs[lx + 1] = pack('<I2I2I2BB', tt & 0xFFFF, mat, fl, liq, (d >> 4) & 7)
              end
              rows[y][bx + 1] = table.concat(recs)
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
local function q(f, u) local ok, r = pcall(f, u) return ok and r or false end
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
  local flags = jobj({
    'citizen', jb(q(U.isCitizen, u)), 'resident', jb(q(U.isResident, u)), 'animal', jb(q(U.isAnimal, u)),
    'tame', jb(q(U.isTame, u)), 'dead', jb(q(U.isDead, u)),
    'hostile', jb(q(U.isInvader, u) or q(U.isUndead, u) or (q(U.isDanger, u) and not q(U.isTame, u))),
    'visitor', jb(q(U.isVisitor, u)), 'merchant', jb(q(U.isMerchant, u)), 'caged', jb(u.flags1.caged),
    'forest', jb(q(U.isForest, u)), 'inactive', jb(not q(U.isActive, u)) })
  return jobj({
    'id', ji(u.id), 'race', ja(cr.creature_id), 'caste', ja(caste.caste_id), 'name', js(name), 'name_native', js(native),
    'profession', ja(df.profession[u.profession] or ''), 'custom_profession', js(u.custom_profession),
    'pos', jpos(u.pos.x, u.pos.y, u.pos.z), 'flags', flags, 'civ_id', ji(u.civ_id), 'hist_fig_id', ji(u.hist_figure_id),
    'sex', ji(u.sex), 'age', jf(try(U.getAge, u, true) or 0),
    'hunger_timer', ji(u.counters2.hunger_timer), 'thirst_timer', ji(u.counters2.thirst_timer),
    'sleepiness_timer', ji(u.counters2.sleepiness_timer), 'stress', ji(stress),
    'skills', jarr(skills), 'labors', jarr(labors), 'phys_attrs', jarr(phys), 'ment_attrs', jarr(ment),
    'facets', jarr(facets), 'inventory', jarr(inv), 'squad_id', ji(u.military.squad_id), 'mood', ja(mood),
    'body_size', ji(try(function() return u.body.size_info.size_cur end) or 0), 'walk_speed', ji(900),
    'profession_name', js(try(U.getProfessionName, u) or ''), 'readable_name', js(try(U.getReadableName, u) or '') })
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
    if i % 16 == 0 then phase_detail = i .. '/' .. #ids; checkpoint() end
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
local function item_json(it, x, y, z)
  local t = it:getType()
  local subtype = ''
  if it:getSubtype() >= 0 then subtype = try(function() return it.subtype.id end) or '' end
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
  return jobj({ 'id', ji(it.id), 'kind', ja(df.item_type[t] or tostring(t)), 'subtype', ja(subtype),
                'mat', ji(palidx(it:getMaterial(), it:getMaterialIndex())), 'pos', jpos(x, y, z),
                'stack', ji(try(function() return it:getStackSize() end) or 1), 'quality', ji(math.max(0, math.min(255, quality))),
                'wear', ji(math.max(0, math.min(255, wear))), 'flags', flags,
                'container', ji(cont and cont.id or -1), 'holder', ji(holder and holder.id or -1),
                'building', ji(bld and bld.id or -1), 'name', js(name), 'desc', js(desc),
                'value', ji(try(dfhack.items.getValue, it) or 0) })
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
  return jobj({ 'id', ji(bld.id), 'kind', ja(BT[t] or tostring(t)), 'subtype', ja(subtype_name(bld, t) or ''),
                'x1', ji(bld.x1), 'y1', ji(bld.y1), 'x2', ji(bld.x2), 'y2', ji(bld.y2), 'z', ji(bld.z),
                'center', jpos(bld.centerx, bld.centery, bld.z), 'mat', ji(palidx(bld.mat_type, bld.mat_index)),
                'stage', ji(stage), 'built', jb(stage >= maxst), 'owner', ji(owner), 'name', js(name),
                'stockpile', stockpile, 'extents', jarr(ext), 'items', jarr(items) })
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
  phase = 'materials'
  local mats = {}
  local i = 1
  while i <= #pal do       -- palette may still grow? no: all palidx calls are done; loop by index anyway
    mats[#mats + 1] = material_json(pal[i][1], pal[i][2])
    if i % 32 == 0 then phase_detail = i .. '/' .. #pal; checkpoint() end
    i = i + 1
  end
  write_file(OUT .. '/materials.json', '[\n' .. table.concat(mats, ',\n') .. '\n]\n')
  counts.materials = #mats
  for k, v in pairs(unresolved) do note(('map: %d %s tiles without resolvable material'):format(v, k)) end
  for k, v in pairs(warn_count) do if v > 3 then note(('%s: %d errors in total'):format(k, v)) end end
  note('walk_speed is always 900 (gait raws not reachable from caste_raw in 53.16 DFHack structures)')
  note('LAVA_STONE tiles -> INORGANIC:OBSIDIAN; TREE/ROOT/MUSHROOM tiles -> wood of the nearest tree (approx.)')
  note('GRASS tiles -> STRUCTURAL material of the grass event with the largest amount; PLANT tiles -> plant at the tile')
end

local function finish()
  local T1 = now()
  local wd = df.global.world.world_data
  local mpz = {}
  pcall(function() mpz = { df.global.world.map.region_x, df.global.world.map.region_y, df.global.world.map.region_z } end)
  local cnt = {}
  for _, k in ipairs({ 'units', 'citizens', 'items', 'buildings', 'plants', 'materials', 'tiletypes', 'tiles', 'z_exported' }) do
    cnt[#cnt + 1] = k
    cnt[#cnt + 1] = ji(counts[k] or 0)
  end
  local nl = {}
  for _, s in ipairs(notes) do nl[#nl + 1] = js(s) end
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
  local ok, err = coroutine.resume(co)
  local dt = now() - t
  frames = frames + 1
  total_lua = total_lua + dt
  if dt > max_slice then max_slice = dt end
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
