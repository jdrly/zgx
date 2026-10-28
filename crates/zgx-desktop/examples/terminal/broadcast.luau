-- Broadcast input, as iTerm2's (Shell › Broadcast Input): what is typed into
-- one pane goes to others too, to every pane of every tab, the panes of one
-- tab, or panes picked one by one. Each terminal encodes the keys for its
-- own program (ui.broadcast_input), and the panes it reaches show it.
local state = require("./state")
local sessions = require("./sessions")
local actions = require("./actions")
local layout = require("@lib/layout")

local mode, picked, source = state.broadcast, state.broadcast_panes, state.broadcast_source

local M = {}

-- The terminal panes broadcasting now, by pane.
M.members = ui.computed(function(): { [string]: boolean }
	local m = mode:get()
	local out: { [string]: boolean } = {}
	if m == "off" then
		return out
	end
	local kinds = state.kinds:get()
	local tab = state.broadcast_tab:get()
	local chosen = picked:get()
	for _, t in state.tabs:get() do
		for _, pane in layout.leaves(t.root) do
			local member = m == "all" or (m == "tab" and t.id == tab) or (m == "panes" and chosen[pane] == true)
			-- Only terminals hear keys this way.
			if member and kinds[pane] == nil then
				out[pane] = true
			end
		end
	end
	return out
end)

-- The members to zgx: their terminals, and the source when it is one.
ui.effect(function()
	local members = M.members:get()
	local terms = {}
	for pane in members do
		table.insert(terms, ui.term(pane))
	end
	local from = source:get()
	ui.broadcast_input(terms, if members[from] then ui.term(from) else nil)
end)

-- The focused pane, tracked.
local function current(): string?
	return (state.focus:get())[state.active:get()]
end

-- Send Input to Current Session Only.
function M.only()
	mode:set("off")
	picked:set {}
	source:set("")
end
-- Broadcast Input to All Panes in All Tabs: chosen again, it stops.
function M.all()
	if mode:peek() == "all" then
		M.only()
	else
		mode:set("all")
	end
end
-- Broadcast Input to All Panes in Current Tab: chosen again, it stops.
function M.tab()
	local tab = state.active:peek()
	if mode:peek() == "tab" and state.broadcast_tab:peek() == tab then
		M.only()
	else
		state.broadcast_tab:set(tab)
		mode:set("tab")
	end
end
-- Toggle Broadcast Input to Current Session: the focused pane joins or
-- leaves; from a whole tab or every tab, their panes are where it starts.
function M.toggle_current()
	local pane = actions.current()
	if not pane or not sessions.is_terminal(pane) then
		return
	end
	local set: { [string]: boolean } = {}
	for p in (if mode:peek() == "panes" then picked:peek() else M.members:peek()) do
		set[p] = true
	end
	set[pane] = if set[pane] then nil else true
	if next(set) == nil then
		M.only()
		return
	end
	picked:set(set)
	mode:set("panes")
end
-- Current Session is Broadcast Source: only its keys go to the rest.
function M.toggle_source()
	local pane = actions.current()
	if pane then
		source:set(if source:peek() == pane then "" else pane)
	end
end
function M.toggle_indicator()
	state.broadcast_indicator:set(not state.broadcast_indicator:peek())
end

-- The menu's check marks.
M.checked = {
	only = function()
		return mode:get() == "off"
	end,
	all = function()
		return mode:get() == "all"
	end,
	tab = function()
		return mode:get() == "tab" and state.broadcast_tab:get() == state.active:get()
	end,
	current = function()
		local pane = current()
		return mode:get() == "panes" and pane ~= nil and (picked:get())[pane] == true
	end,
	source = function()
		local pane = current()
		return pane ~= nil and source:get() == pane
	end,
	indicator = function()
		return state.broadcast_indicator:get()
	end,
}

-- Whether `pane` shows that it broadcasts.
function M.shown(pane: string): boolean
	return state.broadcast_indicator:get() and (M.members:get())[pane] == true
end

return M
