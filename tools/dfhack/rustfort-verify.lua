-- rustfort-verify.lua - READ-ONLY spot check of an export against the live game.
--   dfhack-run lua -f <abs path>/rustfort-verify.lua <samples.txt> <result.txt>
-- samples.txt (written by `cargo run -p rf-import --example spotcheck -- pick ...`):
--   tile X Y Z      -> "tile X Y Z <tiletype name> <material token>"
--   unit ID         -> "unit ID <x> <y> <z> <profession>|<translated name>"
-- plus "count <key> <n>" lines for citizens/units/items/buildings at verification time, and (round 2)
-- "orders N", "order ID JOB LEFT TOTAL FREQ NCONDS", "wsjobs BLD N" (via the global job list),
-- "farm BLD C1,C2,C3,C4" ('-' = fallow), "mod I ID:VERSION", "plant_raws N".
-- Uses plain field access (no raw memory copies) so it checks the exporter's fast path.
-- A few hundred samples take a few ms; no pause, no state change.

local args = { ... }
local inp, outp = args[1], args[2]
if not inp or not outp then print('usage: rustfort-verify.lua <samples.txt> <result.txt>') return end
local raws = df.global.world.raws
local function try(f, ...) local ok, a = pcall(f, ...) if ok then return a end end
local function token(mt, mi)
  if mt == nil or mt < 0 then return 'NONE' end
  return try(function() return dfhack.matinfo.decode(mt, mi):getToken() end) or ('?' .. mt .. ':' .. tostring(mi))
end
local TM = df.tiletype_material
local function layer_token(x, y, z)
  local d = dfhack.maps.getTileBlock(x, y, z).designation[x % 16][y % 16]
  local rx, ry = dfhack.maps.getTileBiomeRgn(x, y, z)
  local geo = dfhack.maps.getRegionBiome(rx, ry).geo_index
  return token(0, df.global.world.world_data.geo_biomes[geo].layers[d.geolayer_index].mat_index)
end
local function plant_token(p, suffix)
  local id = raws.plants.all[p.material].id
  local t = 'PLANT:' .. id .. ':' .. suffix
  if suffix == 'WOOD' and not try(dfhack.matinfo.find, t) then t = 'PLANT:' .. id .. ':STRUCTURAL' end
  return t
end
local function tile_token(x, y, z)
  local tt = dfhack.maps.getTileType(x, y, z)
  if not tt then return 'NOBLOCK', 'NONE' end
  local cls = df.tiletype.attrs[tt].material
  local blk = dfhack.maps.getTileBlock(x, y, z)
  local lx, ly = x % 16, y % 16
  local tok = 'NONE'
  if cls == TM.STONE or cls == TM.SOIL or cls == TM.POOL or cls == TM.BROOK or cls == TM.RIVER then
    tok = layer_token(x, y, z)
  elseif cls == TM.MINERAL then
    tok = nil
    for _, ev in ipairs(blk.block_events) do
      if df.block_square_event_mineralst:is_instance(ev) and (ev.tile_bitmask.bits[ly] & (1 << lx)) ~= 0 then
        tok = token(0, ev.inorganic_mat)
      end
    end
    tok = tok or layer_token(x, y, z)
  elseif cls == TM.CONSTRUCTION then
    local pos = df.coord:new()   -- df.coord:new(x, y, z) is rejected by DFHack
    pos.x, pos.y, pos.z = x, y, z
    local c = df.construction.find(pos)
    df.delete(pos)
    tok = c and token(c.mat_type, c.mat_index) or 'NONE'
  elseif cls == TM.TREE or cls == TM.ROOT or cls == TM.MUSHROOM then
    local p = dfhack.maps.getPlantAtTile(x, y, z)
    tok = p and plant_token(p, 'WOOD') or 'NONE'
  elseif cls == TM.PLANT then
    local p = dfhack.maps.getPlantAtTile(x, y, z)
    tok = p and plant_token(p, 'STRUCTURAL') or 'NONE'
  elseif cls == TM.GRASS_LIGHT or cls == TM.GRASS_DARK or cls == TM.GRASS_DRY or cls == TM.GRASS_DEAD then
    local best, bi = 0, nil
    for _, ev in ipairs(blk.block_events) do
      if df.block_square_event_grassst:is_instance(ev) and ev.amount[lx][ly] > best then
        best, bi = ev.amount[lx][ly], ev.plant_index
      end
    end
    tok = bi and ('PLANT:' .. raws.plants.all[bi].id .. ':STRUCTURAL') or 'NONE'
  elseif cls == TM.FROZEN_LIQUID then tok = 'WATER'
  elseif cls == TM.LAVA_STONE or cls == TM.MAGMA then tok = 'INORGANIC:OBSIDIAN'
  elseif cls == TM.FEATURE then
    local d = blk.designation[lx][ly]
    if d.feature_local then
      local fi = try(dfhack.maps.getLocalInitFeature, blk.region_pos, blk.local_feature)
      tok = fi and try(function() return token(fi.mat_type, fi.mat_index) end) or 'INORGANIC:RAW_ADAMANTINE'
    else
      local fi = try(dfhack.maps.getGlobalInitFeature, blk.global_feature)
      tok = fi and try(function() return token(fi.mat_type, fi.mat_index) end) or 'INORGANIC:SLADE'
    end
  end
  return df.tiletype[tt] or tostring(tt), tok
end

local out = {}
for line in io.lines(inp) do
  local kind, a, b, c = line:match('^(%a+)%s+(-?%d+)%s*(-?%d*)%s*(-?%d*)')
  if kind == 'tile' then
    local x, y, z = tonumber(a), tonumber(b), tonumber(c)
    local ok, nm, tok = pcall(tile_token, x, y, z)
    out[#out + 1] = ('tile %d %d %d %s %s'):format(x, y, z, ok and nm or 'ERROR', ok and tok or tostring(nm))
  elseif kind == 'unit' then
    local u = df.unit.find(tonumber(a))
    if u then
      local name = dfhack.df2utf(dfhack.translation.translateName(dfhack.units.getVisibleName(u), true))
      out[#out + 1] = ('unit %d %d %d %d %s|%s'):format(u.id, u.pos.x, u.pos.y, u.pos.z, df.profession[u.profession], name)
    else
      out[#out + 1] = ('unit %s gone'):format(a)
    end
  end
end
-- counts now (the export used the same filters)
local U = dfhack.units
local units, cit = 0, 0
for _, u in ipairs(df.global.world.units.all) do
  if not U.isDead(u) and (U.isActive(u) or U.isCitizen(u) or U.isResident(u)) then units = units + 1 end
  if U.isCitizen(u) and not U.isDead(u) then cit = cit + 1 end
end
local items = 0
for _, it in ipairs(df.global.world.items.all) do
  if not it.flags.removed and not it.flags.garbage_collect then
    local x = dfhack.items.getPosition(it)
    if x ~= nil then
      local px = type(x) == 'number' and x or x.x
      if px >= 0 then items = items + 1 end
    end
  end
end
out[#out + 1] = 'count units ' .. units
out[#out + 1] = 'count citizens ' .. cit
out[#out + 1] = 'count items ' .. items
out[#out + 1] = 'count buildings ' .. #df.global.world.buildings.all
out[#out + 1] = 'count plants ' .. #df.global.world.plants.all
out[#out + 1] = 'year_tick ' .. df.global.cur_year .. ' ' .. df.global.cur_year_tick

-- round 2: work orders, workshop job queues, farm crops, mods (compared by `spotcheck compare`)
local orders = df.global.world.manager_orders.all
out[#out + 1] = 'orders ' .. #orders
for _, o in ipairs(orders) do
  out[#out + 1] = ('order %d %s %d %d %s %d'):format(o.id, df.job_type[o.job_type], o.amount_left, o.amount_total,
    df.workquota_frequency_type[o.frequency] or tostring(o.frequency), #o.item_conditions)
end
-- jobs per workshop/furnace through a different path than the exporter: the global job list + job holder ref
local per_bld = {}
local link = df.global.world.jobs.list.next
local JT = df.job_type
while link do
  local j = link.item
  if j and j.job_type ~= JT.ConstructBuilding and j.job_type ~= JT.DestroyBuilding then
    local b = try(dfhack.job.getHolder, j)
    if b and (b:getType() == df.building_type.Workshop or b:getType() == df.building_type.Furnace) then
      per_bld[b.id] = (per_bld[b.id] or 0) + 1
    end
  end
  link = link.next
end
for _, b in ipairs(df.global.world.buildings.all) do
  local t = b:getType()
  if t == df.building_type.Workshop or t == df.building_type.Furnace then
    out[#out + 1] = ('wsjobs %d %d'):format(b.id, per_bld[b.id] or 0)
  elseif t == df.building_type.FarmPlot then
    local c = {}
    for s = 0, 3 do
      local pid = b.plant_id[s]
      c[#c + 1] = pid >= 0 and raws.plants.all[pid].id or '-'
    end
    out[#out + 1] = ('farm %d %s'):format(b.id, table.concat(c, ','))
  end
end
local ol = df.global.world.object_loader
for i = 0, #ol.object_load_order_id - 1 do
  out[#out + 1] = ('mod %d %s:%d'):format(i, ol.object_load_order_id[i].value, ol.object_load_order_numeric_version[i])
end
out[#out + 1] = 'plant_raws ' .. #raws.plants.all
local f = assert(io.open(outp, 'wb'))
f:write(table.concat(out, '\n'), '\n')
f:close()
print(('rustfort-verify: %d lines -> %s'):format(#out, outp))
