-- rustfort-probe.lua - READ-ONLY: prints one JSON line about the loaded game, for Rustfort's
-- "Import from Dwarf Fortress" dialog.  DF 53.16 + DFHack 53.16-r2.
--
--   dfhack-run lua -f <abs path>/rustfort-probe.lua
--
-- It only reads a few globals (map loaded, fortress mode, site and world names, date, citizen
-- count, pause state). It never pauses or unpauses the game, never writes to DF memory, and
-- enables nothing.

local function utf(s)
  if s == nil then return '' end
  s = tostring(s)
  if s:find('[\128-\255]') then s = s:gsub('[\128-\255]+', dfhack.df2utf) end
  return s
end
local ESC = { ['"'] = '\\"', ['\\'] = '\\\\', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
local function js(s)
  return '"' .. utf(s):gsub('[%c"\\]', function(c) return ESC[c] or string.format('\\u%04x', c:byte()) end) .. '"'
end
local function jb(v) return v and 'true' or 'false' end

if not dfhack.isMapLoaded() then
  print('{"ok":true,"map_loaded":false,"dfhack":' .. js(dfhack.getDFHackVersion()) .. '}')
  return
end

local fort_mode = false
pcall(function() fort_mode = dfhack.world.isFortressMode() end)
local site = nil
pcall(function() site = df.global.world.world_data.active_site[0] end)
if not fort_mode or not site then
  print('{"ok":true,"map_loaded":true,"fort_mode":false,"dfhack":' .. js(dfhack.getDFHackVersion()) .. '}')
  return
end

local function name(n, english)
  local ok, s = pcall(dfhack.translation.translateName, n, english)
  return ok and s or ''
end
local citizens = 0
pcall(function()
  for _, u in ipairs(df.global.world.units.active) do
    if dfhack.units.isCitizen(u) and not dfhack.units.isDead(u) then citizens = citizens + 1 end
  end
end)
local paused = false
pcall(function() paused = df.global.pause_state end)

print(('{"ok":true,"map_loaded":true,"fort_mode":true,"fort":%s,"fort_native":%s,"world":%s,"year":%d,"year_tick":%d,"citizens":%d,"paused":%s,"dfhack":%s}'):format(
  js(name(site.name, true)), js(name(site.name, false)), js(name(df.global.world.world_data.name, false)),
  df.global.cur_year, df.global.cur_year_tick, citizens, jb(paused), js(dfhack.getDFHackVersion())))
