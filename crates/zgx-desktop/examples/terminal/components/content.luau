-- The active tab's panes and dividers. A pane or a tab dragged over them
-- shows, live, where it would go; dropping keeps it.
local theme = require("../theme")
local state = require("../state")
local actions = require("../actions")
local frames = require("../frames")
local Pane = require("./pane")
local Divider = require("./divider")

local active, drop = state.active, state.drop

type Placed = { id: string, tab: string }
-- Each mounted pane with the tab it is in, so one that changes tab is built
-- again: a tab dropped beside a pane shows its pane here before the drop,
-- while it is still the other tab's, and the drop changes no pane shown.
local placed = ui.computed(function(): { Placed }
	local _ = state.tabs:get()
	local out: { Placed } = {}
	for _, id in frames.mounted_ids:get() do
		table.insert(out, { id = id, tab = actions.tab_of(id) or "" })
	end
	return out
end)

return function(): Node
	return ui.div {
		grow = 1,
		min_h = 0,
		mx = theme.inset,
		mb = theme.inset,
		-- The box the panes' insets count from (not the lists' own nodes).
		relative = true,
		on_mount = function(el)
			frames.set_area(el)
		end,
		-- The active tab itself comes here lifted out of the strip, when
		-- another can show.
		drop_when = function(payload: state.Payload)
			return payload.kind == "pane"
				or (payload.kind == "tab" and (payload.id ~= active:peek() or #state.tabs:peek() > 1))
		end,
		on_drag_over = function(payload: state.Payload, e)
			actions.hide_slot()
			if payload.kind == "tab" then
				actions.lift(payload.id)
			end
			local target, zone = frames.target_at(e.local_x, e.local_y)
			if target == payload.id then
				target = nil
			end
			local d = drop:peek()
			if d.target ~= target or d.zone ~= zone or d.id ~= payload.id then
				drop:set(if target then { target = target, zone = zone, kind = payload.kind, id = payload.id } else {})
			end
		end,
		on_drag_leave = function()
			if (drop:peek()).target then
				drop:set {}
			end
		end,
		on_drop = function()
			local d = drop:peek()
			drop:set {}
			frames.commit_drop(d)
		end,
		ui.each(placed, function(item: Placed)
			return Pane { pane = item.id }
		end, {
			-- A pane moved to another tab is built again there.
			key = function(item: Placed)
				return `{item.id}@{item.tab}`
			end,
		}),
		ui.each(frames.divider_ids, function(id: string)
			return Divider { id = id }
		end, {
			key = function(id: string)
				return id
			end,
		}),
	}
end
