-- A reply's markdown, drawn from `ui.markdown`'s blocks: paragraphs and
-- headings of styled runs, lists, quotes, tables, and code blocks with
-- their language and a Copy button. Keyed by the message, a reply that
-- grows parses only its new end, and only the blocks that changed draw
-- again.

-- A run of text: `x`, and whether it is bold, italic, struck, code or a
-- link (`a`, where to).
type Run = { x: string, b: boolean?, i: boolean?, s: boolean?, c: boolean?, a: string? }

local function runs(list: { Run }, props: RichProps?): Node
	local rich: RichProps = props or { wrap = true }
	for _, r in list do
		table.insert(
			rich,
			ui.span {
				r.x,
				literal = true,
				bold = r.b or nil,
				italic = r.i or nil,
				strike = r.s or nil,
				font = if r.c then "mono" else nil,
				bg = if r.c then "$surface.sunken" else nil,
				padding = if r.c then { 3, 0.5 } else nil,
				radius = if r.c then 3 else nil,
				href = r.a,
			}
		)
	end
	return ui.rich(rich)
end

local block: (b: MarkdownBlock) -> Node

local function code(b: MarkdownBlock): Node
	local copied = ui.use_state(false)
	local lang: string = b.lang or ""
	return ui.column {
		role = "group",
		label = if lang ~= "" then `{lang} code` else "Code",
		w_full = true,
		bg = "$surface.sunken",
		rounded = 8,
		border = 1,
		border_color = "$border.subtle",
		ui.row {
			w_full = true,
			px = 12,
			py = 4,
			items = "center",
			selectable = false,
			ui.text {
				if lang ~= "" then lang else "text",
				literal = true,
				color = "$text.tertiary",
				text_size = "$font.size.sm",
				grow = 1,
			},
			ui.button {
				id = "copy",
				function()
					return if copied:get() then "Copied" else "Copy"
				end,
				px = 8,
				py = 3,
				bg = "#00000000",
				text_size = "$font.size.sm",
				on_click = function()
					ui.clipboard.write(b.text)
					copied:set(true)
					ui.after(1500, function()
						copied:set(false)
					end)
				end,
			},
		},
		ui.scroll_x {
			w_full = true,
			px = 12,
			pb = 10,
			ui.text { b.text, literal = true, font = "mono", text_size = 13, shrink = 0 },
		},
	}
end

function block(b: MarkdownBlock): Node
	if b.t == "h" then
		return runs(b.s, {
			wrap = true,
			heading = b.l,
			text_size = if b.l == 1 then "$font.size.xl" else "$font.size.lg",
			bold = true,
			pt = 4,
		})
	elseif b.t == "code" then
		return code(b)
	elseif b.t == "ul" or b.t == "ol" then
		local items: { Child } = {}
		for i, item in b.items do
			local inner: { Child } = {}
			for _, child in item do
				table.insert(inner, block(child))
			end
			table.insert(
				items,
				ui.row {
					role = "listitem",
					gap = 8,
					ui.text {
						if b.t == "ol" then `{(b.start or 1) + i - 1}.` else "•",
						literal = true,
						color = "$text.tertiary",
						aria_hidden = true,
						shrink = 0,
					},
					ui.column { grow = 1, min_w = 0, gap = 4, inner },
				}
			)
		end
		return ui.column { role = "list", gap = 4, items }
	elseif b.t == "quote" then
		local inner: { Child } = {}
		for _, child in b.b do
			table.insert(inner, block(child))
		end
		return ui.column {
			pl = 12,
			gap = 6,
			border_sides = { left = 3 },
			border_color = "$border",
			color = "$text.secondary",
			inner,
		}
	elseif b.t == "hr" then
		return ui.div { w_full = true, h = 1, my = 4, bg = "$border.subtle" }
	elseif b.t == "table" then
		local rows: { Child } = {}
		local function line(cells: { any }, bold: boolean)
			local row: { Child } = {}
			for _, cell in cells do
				table.insert(row, ui.column { w = 160, runs(cell, { wrap = true, bold = bold or nil }) })
			end
			table.insert(rows, ui.row { gap = 12, row })
		end
		line(b.head, true)
		for _, cells in b.rows do
			line(cells, false)
		end
		return ui.column { gap = 4, rows }
	end
	return runs(b.s or {})
end

-- `text()`'s markdown, parsed under `key` (the message's id).
return function(text: () -> string, key: string): Node
	local blocks = ui.computed(function()
		return ui.markdown(text(), { key = key })
	end)
	return ui.column {
		w_full = true,
		gap = 10,
		ui.each(blocks, block),
	}
end
