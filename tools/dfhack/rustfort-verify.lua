-- rustfort-verify.lua - READ-ONLY spot check of an export against the live game.
--   dfhack-run lua -f <abs path>/rustfort-verify.lua <samples.txt> <result.txt>
-- samples.txt (written by `cargo run -p rf-import --example spotcheck -- pick ...`):
--   tile X Y Z      -> "tile X Y Z <tiletype name> <material token>"
--   unit ID         -> "unit ID <x> <y> <z> <profession>|<translated name>"
-- plus "count <key> <n>" lines for citizens/units/items/buildings at verification time, and (round 2)
-- "orders N", "order ID JOB LEFT TOTAL FREQ NCONDS", "wsjobs BLD N" (via the global job list),
-- "farm BLD C1,C2,C3,C4" ('-' = fallow), "mod I ID:VERSION", "plant_raws N".
-- Round 3 samples: "psy ID" (needs/emotions/values/relationship/wound counts, blood, squad, civ, stress),
-- "aq X Y Z" (water_table + heavy_aquifer), "btile BID X Y Z" (burrow membership), "corpse ITEM" (race token);
-- plus "squad", "burrow", "itemdefs", "craw" (creature bodies) and "mech" (material mechanics) lines.
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
  elseif kind == 'psy' then
    -- round 3: psychology/relationship/wound counts, blood, squad, invader civ of one unit
    local u = df.unit.find(tonumber(a))
    if u then
      local p = try(function() return u.status.current_soul.personality end)
      local h = u.hist_figure_id >= 0 and df.historical_figure.find(u.hist_figure_id) or nil
      local fam = h and #h.histfig_links or 0
      local vis = h and (try(function() return #h.info.relationships.hf_visual end) or 0) or 0
      local wl = 0
      for _, w in ipairs(u.body.wounds) do wl = wl + math.max(1, #w.parts) end
      out[#out + 1] = ('psy %d needs %d emotions %d values %d family %d visual %d wounds %d blood %d %d squad %d %d civ %d stress %d')
        :format(u.id, p and #p.needs or 0, p and #p.emotions or 0, p and #p.values or 0, fam, vis, wl,
                u.body.blood_count, u.body.blood_max, u.military.squad_id, u.military.squad_position, u.civ_id,
                p and p.stress or 0)
    else
      out[#out + 1] = ('psy %s gone'):format(a)
    end
  elseif kind == 'aq' then
    -- round 3: aquifer flags through getTileFlags (exporter: raw block copies)
    local x, y, z = tonumber(a), tonumber(b), tonumber(c)
    local d, o = dfhack.maps.getTileFlags(x, y, z)
    if d then
      out[#out + 1] = ('aq %d %d %d %d %d'):format(x, y, z, d.water_table and 1 or 0, o.heavy_aquifer and 1 or 0)
    end
  elseif kind == 'btile' then
    -- round 3: burrow membership through dfhack.burrows.isAssignedTile (exporter: block masks)
    local bid, x, y, z = tonumber(a), line:match('^btile%s+%-?%d+%s+(%-?%d+)%s+(%-?%d+)%s+(%-?%d+)')
    x, y, z = tonumber(x), tonumber(y), tonumber(z)
    local bur
    for _, bb in ipairs(df.global.plotinfo.burrows.list) do if bb.id == bid then bur = bb end end
    if bur then
      local pos = df.coord:new()
      pos.x, pos.y, pos.z = x, y, z
      local inb = dfhack.burrows.isAssignedTile(bur, pos)
      df.delete(pos)
      out[#out + 1] = ('btile %d %d %d %d %d'):format(bid, x, y, z, inb and 1 or 0)
    end
  elseif kind == 'corpse' then
    local it = df.item.find(tonumber(a))
    if it then
      out[#out + 1] = ('corpse %d %s'):format(it.id, try(function() return raws.creatures.all[it.race].creature_id end) or '?')
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

-- round 3: squads, burrows, item raws, creature bodies, material mechanics
local gid = df.global.plotinfo.group_id
for _, s in ipairs(df.global.world.squads.all) do
  if s.entity_id == gid then
    local filled = 0
    for _, p in ipairs(s.positions) do if p.occupant >= 0 then filled = filled + 1 end end
    local rname = '?'
    for _, r in ipairs(df.global.plotinfo.alerts.routines) do if r.id == s.cur_routine_idx then rname = r.name end end
    out[#out + 1] = ('squad %d %d %d %d %s'):format(s.id, #s.positions, filled, #s.orders, rname)
  end
end
for _, b in ipairs(df.global.plotinfo.burrows.list) do
  -- tile count via dfhack.burrows.listBlocks + isAssignedBlockTile (exporter: the raw link list)
  local n = 0
  for _, blk in ipairs(dfhack.burrows.listBlocks(b)) do
    for lx = 0, 15 do for ly = 0, 15 do
      if dfhack.burrows.isAssignedBlockTile(b, blk, lx, ly) then n = n + 1 end
    end end
  end
  out[#out + 1] = ('burrow %d %d %d'):format(b.id, n, #b.units)
end
local W = raws.itemdefs
out[#out + 1] = ('itemdefs %d %d'):format(#W.weapons + #W.ammo + #W.trapcomps,
  #W.armor + #W.helms + #W.gloves + #W.shoes + #W.pants + #W.shields)
for _, tok in ipairs({ 'DWARF', 'GOBLIN', 'HUMAN', 'ELF', 'CAT' }) do
  for _, cr in ipairs(raws.creatures.all) do
    if cr.creature_id == tok then
      local c = cr.caste[0]
      local layers = 0
      for _, p in ipairs(c.body_info.body_parts) do layers = layers + #p.layers end
      out[#out + 1] = ('craw %s %d %d %d %d %d'):format(tok, #cr.caste, #c.body_info.body_parts, layers, #c.body_info.attacks,
        c.body_size[#c.body_size - 1] * 10)
    end
  end
end
for _, tok in ipairs({ 'INORGANIC:IRON', 'INORGANIC:STEEL', 'INORGANIC:COPPER', 'INORGANIC:GRANITE', 'CREATURE:DWARF:BONE' }) do
  local m = try(function() return dfhack.matinfo.find(tok).material end)
  if m then
    out[#out + 1] = ('mech %s %d %d %d %d'):format(tok, m.solid_density, m.strength.yield[df.strain_type.SHEAR],
      m.strength.fracture[df.strain_type.IMPACT], m.strength.max_edge)
  end
end
-- round 4: building states and links, nobles, settings, entities, caravans - through other paths than
-- the exporter where DF offers one (raw flag words, linked_mechanisms, dfhack.units.getNoblePositions)
for _, b in ipairs(df.global.world.buildings.all) do
  local t = df.building_type[b:getType()]
  if t == 'Door' or t == 'Hatch' then
    local wd = b.door_flags.whole   -- bit 0 forbidden (= locked), bit 4 closed
    out[#out + 1] = ('door %d %s %s'):format(b.id, (wd & 1) ~= 0 and 'LOCKED' or '-', (wd & 16) ~= 0 and 'CLOSED' or 'OPEN')
  elseif t == 'Bridge' then
    out[#out + 1] = ('bridge %d %s %d'):format(b.id, (b.gate_flags.whole & 1) ~= 0 and 'RAISED' or 'LOWERED', b.direction)
  elseif t == 'Trap' then
    local tt = df.trap_type[b.trap_type]
    if tt == 'Lever' or tt == 'PressurePlate' then
      -- targets: the building holding each linked mechanism (exporter: the TRIGGERTARGET refs of its own items)
      local tg = {}
      for _, it in ipairs(b.linked_mechanisms) do
        local h = dfhack.items.getHolderBuilding(it)
        if h then tg[#tg + 1] = h.id end
      end
      table.sort(tg)
      out[#out + 1] = ('lever %d %d %s'):format(b.id, b.state, #tg > 0 and table.concat(tg, ',') or '-')
    else
      local loaded = 0
      for _, ci in ipairs(b.contained_items) do
        local it = ci.item
        local ity = df.item_type[it:getType()]
        if ity == 'WEAPON' or ity == 'TRAPCOMP' or ity == 'BOULDER' then loaded = 1 end
        if ity == 'CAGE' then
          loaded = 1
          for _, r in ipairs(it.general_refs) do if df.general_ref_contains_unitst:is_instance(r) then loaded = 0 end end
        end
      end
      out[#out + 1] = ('trap %d %s %d'):format(b.id, tt, loaded)
    end
  end
end
for _, u in ipairs(df.global.world.units.active) do
  if dfhack.units.isCitizen(u) then
    for _, np in ipairs(dfhack.units.getNoblePositions(u) or {}) do
      out[#out + 1] = ('noblepos %d %s'):format(u.id, np.position.code)
    end
  end
end
do
  local dw = df.global.d_init.dwarf
  out[#out + 1] = ('settings %d %d %d'):format(dw.population_cap, dw.strict_population_cap, dw.baby_cap_absolute)
  local civ = df.historical_entity.find(df.global.plotinfo.civ_id)
  for _, s in ipairs(civ.relations.diplomacy.state) do out[#out + 1] = ('diplomacy %d %d'):format(s.group_id, s.relation) end
  out[#out + 1] = ('caravans %d'):format(#df.global.plotinfo.caravans)
  out[#out + 1] = ('mandates %d'):format(#df.global.world.mandates.all)
end
-- round 8: work details, kitchen, locations (zones via civzone.location_id, holders via world.occupations),
-- crimes, animals, pregnancies, stockpile links and sampled filter leaves (tokens resolved here independently)
do
  for i, d in ipairs(df.global.plotinfo.labor_info.work_details) do
    local nl = 0
    for l = 0, df.unit_labor._last_item do if try(function() return d.allowed_labors[l] end) then nl = nl + 1 end end
    out[#out + 1] = ('wd %d %s %d %d %s'):format(i, df.work_detail_mode[d.flags.mode] or '?', #d.assigned_units, nl, d.name)
  end
  local k = df.global.plotinfo.kitchen
  local cook, brew = 0, 0
  for i = 0, #k.exc_types - 1 do
    if k.exc_types[i].Cook then cook = cook + 1 end
    if k.exc_types[i].Brew then brew = brew + 1 end
  end
  out[#out + 1] = ('kitchen %d %d %d'):format(#k.item_types, cook, brew)
  local site_id = df.global.plotinfo.site_id
  local zones, holders = {}, {}
  for _, b in ipairs(df.global.world.buildings.all) do
    if b:getType() == df.building_type.Civzone and b.location_id >= 0 then zones[b.location_id] = (zones[b.location_id] or 0) + 1 end
  end
  for _, o in ipairs(df.global.world.occupations.all) do
    if o.site_id == site_id and o.location_id >= 0 and o.unit_id >= 0 then holders[o.location_id] = (holders[o.location_id] or 0) + 1 end
  end
  for _, ab in ipairs(df.global.plotinfo.main.fortress_site.buildings) do
    out[#out + 1] = ('loc %d %s %d %d %d'):format(ab.id, df.abstract_building_type[ab:getType()], zones[ab.id] or 0, holders[ab.id] or 0,
      try(function() return ab.contents.location_value end) or -1)
  end
  local nc, ns = 0, 0
  for _, c in ipairs(df.global.world.crimes.all) do
    if c.site == site_id then nc = nc + 1; if c.flags.sentenced then ns = ns + 1 end end
  end
  out[#out + 1] = ('crimes %d %d'):format(nc, ns)
  for _, u in ipairs(df.global.world.units.active) do
    if dfhack.units.isAnimal(u) and dfhack.units.isTame(u) and not dfhack.units.isDead(u) then
      out[#out + 1] = ('animal %d %d %s %d %d'):format(u.id, u.relationship_ids.PetOwner, df.animal_training_level[u.training_level] or '?',
        u.flags2.slaughter and 1 or 0, u.pregnancy_timer)
    elseif u.pregnancy_timer > 0 and not dfhack.units.isDead(u) then
      out[#out + 1] = ('preg %d %d'):format(u.id, u.pregnancy_timer)
    end
  end
  -- sampled leaves: the first enabled, the first disabled and the middle entry of a few groups per pile
  local MT = raws.mat_table
  local OC = df.organic_mat_category
  local SAMPLE = {
    { 'food', 'meat', function(i) return token(MT.organic_types[OC.Meat][i], MT.organic_indexes[OC.Meat][i]) end },
    { 'food', 'plants', function(i) return token(MT.organic_types[OC.Plants][i], MT.organic_indexes[OC.Plants][i]) end },
    { 'food', 'drink_plant', function(i) return token(MT.organic_types[OC.PlantDrink][i], MT.organic_indexes[OC.PlantDrink][i]) end },
    { 'food', 'seeds', function(i) return token(MT.organic_types[OC.Seed][i], MT.organic_indexes[OC.Seed][i]) end },
    { 'stone', 'mats', function(i) return 'INORGANIC:' .. raws.inorganics.all[i].id end },
    { 'wood', 'mats', function(i) return 'PLANT:' .. raws.plants.all[i].id .. ':WOOD' end },
    { 'furniture', 'type', function(i) return df.furniture_type[i] end },
    { 'refuse', 'type', function(i) return df.item_type[i] end },
    { 'animals', 'enabled', function(i) return raws.creatures.all[i].creature_id end },
    { 'corpses', 'corpses', function(i) return raws.creatures.all[i].creature_id end },
  }
  for _, b in ipairs(df.global.world.buildings.all) do
    if b:getType() == df.building_type.Stockpile then
      out[#out + 1] = ('pile %d %d %d %d'):format(b.id, b.stockpile_flag.use_links_only and 1 or 0,
        #b.links.give_to_pile + #b.links.give_to_workshop, #b.links.take_from_pile + #b.links.take_from_workshop)
      local s = b.settings
      for _, sm in ipairs(SAMPLE) do
        local cat, grp, tokf = sm[1], sm[2], sm[3]
        if s.flags[cat] then
          local v = s[cat][grp]
          local n = #v
          local pick, on1, off1 = {}, nil, nil
          for i = 0, n - 1 do
            local x = v[i] ~= 0 and v[i] ~= false
            if x and not on1 then on1 = i end
            if not x and not off1 then off1 = i end
            if on1 and off1 then break end
          end
          if on1 then pick[#pick + 1] = on1 end
          if off1 then pick[#pick + 1] = off1 end
          if n > 2 then pick[#pick + 1] = n // 2 end
          for _, i in ipairs(pick) do
            local tk = try(tokf, i)
            if tk and tk ~= '' and not tk:find(' ') then
              out[#out + 1] = ('pleaf %d %s %s %d %s'):format(b.id, cat, grp, (v[i] ~= 0 and v[i] ~= false) and 1 or 0, tk)
            end
          end
        end
      end
    end
  end
end
local f = assert(io.open(outp, 'wb'))
f:write(table.concat(out, '\n'), '\n')
f:close()
print(('rustfort-verify: %d lines -> %s'):format(#out, outp))
