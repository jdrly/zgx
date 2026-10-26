//! A box of its own size whose box changed (a sparkline's bar, a progress
//! fill), placed again by its parent without the parent being laid out: a
//! parent of a fixed size lays out its absolute children apart from the rest
//! of its content, so the box's new place is its margins inside the parent's
//! content box, its size its own. Queued as `set_box` changes it, placed in
//! `prepare_layout` where the parent's layout would place it (its place among
//! the layout roots, then its parent's children in order), with the same
//! measuring and arranging, so its bounds and damage come out the same.

use super::*;

impl Scene {
    /// Queue `id`, whose box (`set_box`) or text (`set_kind`) just changed,
    /// what it reads unchanged, to be placed again without its parent's
    /// layout; false where that would not be exact, for the caller to lay it
    /// out as any change is.
    #[inline(always)]
    /// `own`: what changed is more than its box (its text), as
    /// `invalidate_layout_with` takes it.
    pub(super) fn place_later(&mut self, id: NodeId, own: bool) -> bool {
        #[cfg(test)]
        if self.no_placing {
            return false;
        }
        // Most changed nodes are in their parents' flow: no further look.
        self.isolation_count + self.promoted_count + self.relative_count + self.contents_count == 0
            && self.node(id).style.absolute
            && self.place_absolute(id, own)
    }
    /// `place_later` for text `id`, a text before its kind changed: placed
    /// apart if it still is one, its sizes dropped with its text.
    #[cold]
    #[inline(never)]
    pub(super) fn place_text_later(&mut self, id: NodeId) -> bool {
        if !matches!(self.node(id).kind, NodeKind::Text { .. }) || !self.place_later(id, true) {
            return false;
        }
        self.node(id).text_size.set(TextSizes::default());
        true
    }
    #[inline(never)]
    fn place_absolute(&mut self, id: NodeId, own: bool) -> bool {
        let node = self.node(id);
        let style = &node.style;
        let Some(parent) = node.parent else {
            return false;
        };
        // A leaf its parent places apart from its flow: measured again as
        // its parent would measure it, its size its own.
        if !node.children.is_empty()
            || node.contents
            || node.dirty & LAYOUT != 0
            || !style.absolute
            || style.snap
            || node.measurement.is_none()
        {
            return false;
        }
        let owner = self.node(parent);
        // Laid out by the ordinary pass (its taffy tree is kept otherwise),
        // its size its own: what its absolute children change is theirs.
        if owner.dirty & LAYOUT != 0
            || owner.contents
            || owner.style.snap
            || owner.advanced_layout.is_some()
            || !self.is_layout_boundary(parent)
        {
            return false;
        }
        // As `invalidate_layout_with` marks it, its layout aside: on the
        // dirty list where it would be, its ink and its ancestors' cleared;
        // its parent a layout root as the walk would leave it.
        self.invalidate_with(id, 0, true, own);
        // The walk marks its parent too, on the dirty list in its turn.
        self.invalidate_with(parent, 0, true, false);
        if self.layout_roots.last() != Some(&parent) {
            self.layout_roots.push(parent);
        }
        if !std::mem::replace(&mut self.node_mut(id).place_pending, true) {
            self.placing.push(id);
        }
        self.node_mut(parent).placing = true;
        true
    }

    /// Before a layout pass: the queued boxes this pass cannot place apart
    /// (its root lays out the whole tree, or something else changed their
    /// parent or them) are laid out as `set_box` would have had them.
    pub(super) fn settle_placements(&mut self) {
        if self.placing.is_empty() {
            return;
        }
        let whole = self.node(self.root).dirty & LAYOUT != 0;
        for id in std::mem::take(&mut self.placing) {
            if !self.contains(id) || !self.node(id).place_pending {
                continue;
            }
            let parent = self.node(id).parent;
            let apart = !whole
                && self.node(id).dirty & LAYOUT == 0
                && parent.is_some_and(|parent| {
                    self.contains(parent)
                        && self.node(parent).placing
                        && self.node(parent).dirty & LAYOUT == 0
                });
            if apart {
                continue;
            }
            self.node_mut(id).place_pending = false;
            self.node_mut(id).dirty |= LAYOUT;
            if let Some(parent) = parent
                && self.contains(parent)
            {
                let owner = self.node_mut(parent);
                owner.dirty |= LAYOUT;
                owner.placing = false;
            }
        }
    }

    /// Lay out layout root `parent`, which changed only by the boxes queued
    /// in it (`place_later`), as `prepare_layout` would: its result
    /// measured again (its queued children alone change in it), then arranged
    /// where it is. False where it holds none, for the root to be laid out
    /// as any other.
    pub(super) fn place_children(&mut self, parent: NodeId, report: &mut FrameReport) -> bool {
        let owner = self.node_mut(parent);
        if !std::mem::replace(&mut owner.placing, false) || owner.dirty & LAYOUT != 0 {
            return false;
        }
        let Some(last) = self.node(parent).measurement.as_ref() else {
            return false;
        };
        let size = last.size;
        let constraints = last.constraints;
        let count = last.children.len().min(last.child_constraints.len());
        // Another child changed too (a box of its own laid out inside it):
        // laid out whole, the queued ones with it.
        let others = last.children[..count].iter().any(|(child, _)| {
            let node = self.node(*child);
            node.dirty & LAYOUT != 0 && !node.place_pending
        });
        if others {
            for at in 0..count {
                let child = self.node(parent).measurement.as_ref().unwrap().children[at].0;
                let node = self.node_mut(child);
                if std::mem::replace(&mut node.place_pending, false) {
                    node.dirty |= LAYOUT;
                }
            }
            self.node_mut(parent).dirty |= LAYOUT;
            return false;
        }
        // As `measure_uncached` places an absolute child: inside the content
        // box its size leaves.
        let padding = self.content_insets(parent);
        let inner_w = size.0 - (padding.left + padding.right);
        let inner_h = size.1 - (padding.top + padding.bottom);
        let pass = self.layout_pass;
        for at in 0..count {
            let child = self.node(parent).measurement.as_ref().unwrap().children[at].0;
            if !self.node(child).place_pending {
                continue;
            }
            self.node_mut(child).place_pending = false;
            // Changed as `set_box` leaves it: measured again.
            self.node_mut(child).dirty |= LAYOUT;
            let margin = self.node(child).style.margin;
            let placed = Constraints {
                width: Some((inner_w - margin.left - margin.right).max(0.0)),
                height: Some((inner_h - margin.top - margin.bottom).max(0.0)),
                percent_width: Some(inner_w.max(0.0)),
                percent_height: Some(inner_h.max(0.0)),
                ..Constraints::default()
            };
            let ((w, h), placed) = match self.place_box(child, placed) {
                Some(placed) => placed,
                None => self.measure_kept(child, placed),
            };
            let last = self.node_mut(parent).measurement.as_mut().unwrap();
            last.children[at].1 =
                Rect::new(padding.left + margin.left, padding.top + margin.top, w, h);
            last.child_constraints[at] = Some(placed);
        }
        // Its result is this pass's, as `store_measurement` leaves a changed
        // node's: of the others, only this pass's under other constraints.
        let owner = self.node_mut(parent);
        owner.measurement.as_mut().unwrap().pass = pass;
        owner
            .alternates
            .retain(|m| m.constraints != constraints && m.pass == pass);
        owner
            .sizes
            .retain(|m| m.constraints != constraints && m.pass == pass);
        // `arrange` of it where it is: its box as it was, its children
        // placed in order; those that did not change return at once.
        let bounds = owner.bounds;
        let rtl = owner.rtl;
        report.layout_nodes += 1;
        #[cfg(test)]
        {
            self.placed_parents += 1;
        }
        let mut context = None;
        for at in 0..count {
            let (child, mut rect) = self.node(parent).measurement.as_ref().unwrap().children[at];
            let node = self.node(child);
            if !node.arrange_dirty && !node.style.snap && node.dirty & LAYOUT == 0 {
                continue;
            }
            if rtl {
                rect.x = bounds.width - rect.x - rect.width;
            }
            crate::deep(1, || {
                self.arrange(
                    child,
                    bounds.x + rect.x,
                    bounds.y + rect.y,
                    report,
                    false,
                    1,
                    &mut context,
                )
            });
        }
        let owner = self.node_mut(parent);
        owner.dirty &= !LAYOUT;
        owner.arrange_dirty = false;
        true
    }

    /// `measure_kept` for a changed box of its own size (a bar) whose last
    /// result alone is kept: that result written over as `measure_sized_box`
    /// writes it, the lookups `measured` makes first known to find nothing.
    /// None for any other node.
    fn place_box(&mut self, id: NodeId, asked: Constraints) -> Option<((f32, f32), Constraints)> {
        let node = self.node(id);
        let style = &node.style;
        if !matches!(node.kind, NodeKind::Container(_) | NodeKind::Panel { .. })
            || style.layout_options.is_some()
            || style.align == Align::Baseline
            || !node.alternates.is_empty()
            || !node.sizes.is_empty()
            || node.measurement.as_ref()?.pass == self.layout_pass
        {
            return None;
        }
        let constraints = self.relevant_in(id, node, asked);
        let explicit_w = constraints
            .force_width
            .or(style.width)
            .or_else(|| percentage(style.width_percent, constraints.percent_width))
            .map(|w| dimension(w, style.min_width, style.max_width))?;
        let explicit_h = constraints
            .force_height
            .or(style.height)
            .or_else(|| percentage(style.height_percent, constraints.percent_height))
            .map(|h| dimension(h, style.min_height, style.max_height))?;
        let size = (
            dimension(explicit_w, style.min_width, style.max_width),
            dimension(explicit_h, style.min_height, style.max_height),
        );
        let pass = self.layout_pass;
        let node = self.node_mut(id);
        node.advanced_layout = None;
        let last = node.measurement.as_mut()?;
        last.constraints = constraints;
        last.size = size;
        last.pass = pass;
        last.children.clear();
        last.child_constraints.clear();
        node.arrange_dirty = true;
        Some((size, constraints))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two cards of bars (a heavy dashboard's), each frame some bars given
    /// new heights and margins through `set_box`; now and then a card's
    /// absolute label given a new text (placed apart too), its text in flow
    /// changed (the card laid out whole), a card moved or the window resized
    /// (the whole tree): laid out with and without placing apart, every
    /// node's bounds, the damage, the repaints and the layout count the
    /// same, bit for bit.
    #[test]
    fn boxes_placed_apart_lay_out_and_damage_as_their_parents_layout_does() {
        let build = |no_placing: bool| {
            let mut scene = Scene::new(500., 300.);
            scene.no_placing = no_placing;
            let mut bars = Vec::new();
            let mut labels = Vec::new();
            let mut cards = Vec::new();
            let mut bodies = Vec::new();
            for card in 0..2 {
                let panel = scene.append(
                    scene.root(),
                    NodeKind::Container(Layout::Overlay),
                    Style {
                        absolute: true,
                        width: Some(200.),
                        height: Some(90.),
                        padding: 3.5,
                        margin: Insets {
                            left: 10. + card as f32 * 230.,
                            top: 20.25,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                );
                cards.push(panel);
                // In its flow: a change to it lays the card out whole.
                bodies.push(scene.append(
                    panel,
                    NodeKind::Text {
                        text: "body".into(),
                        color: Color(0, 0, 0, 255),
                        font_size: 12.,
                    },
                    Style::default(),
                ));
                labels.push(scene.append(
                    panel,
                    NodeKind::Text {
                        text: "card".into(),
                        color: Color(0, 0, 0, 255),
                        font_size: 11.,
                    },
                    Style {
                        absolute: true,
                        width: Some(100.),
                        height: Some(14.),
                        ..Default::default()
                    },
                ));
                for bar in 0..12 {
                    bars.push(scene.append(
                        panel,
                        NodeKind::Container(Layout::Column),
                        Style {
                            absolute: true,
                            width: Some(5.),
                            height: Some(10.),
                            margin: Insets {
                                left: bar as f32 * 7.5,
                                top: 60.,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    ));
                }
            }
            scene.flush();
            (scene, bars, labels, cards, bodies)
        };
        let (mut a, bars, labels, cards, bodies) = build(false);
        let (mut b, ..) = build(true);
        let mut kept_a = vec![None; bars.len()];
        let mut kept_b = vec![None; bars.len()];
        let mut seed = 0x9e37_79b9_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for frame in 0..200 {
            for (i, bar) in bars.iter().enumerate() {
                if next() % 3 != 0 {
                    continue;
                }
                let height = 1. + (next() % 400) as f32 / 8.;
                let mut style = a.style(*bar);
                style.height = Some(height);
                style.margin.top = 70. - height + (next() % 5) as f32 * 0.3;
                a.set_box(*bar, &style, &mut kept_a[i]);
                b.set_box(*bar, &style, &mut kept_b[i]);
            }
            if frame % 7 == 3 {
                let text: std::sync::Arc<str> = format!("card {frame}").into();
                for scene in [&mut a, &mut b] {
                    scene.set_kind(
                        labels[frame % 2],
                        NodeKind::Text {
                            text: text.clone(),
                            color: Color(0, 0, 0, 255),
                            font_size: 11.,
                        },
                    );
                }
            }
            if frame % 11 == 5 {
                let text: std::sync::Arc<str> = "body ".repeat(frame % 4 + 1).into();
                for scene in [&mut a, &mut b] {
                    scene.set_kind(
                        bodies[frame % 2],
                        NodeKind::Text {
                            text: text.clone(),
                            color: Color(0, 0, 0, 255),
                            font_size: 12.,
                        },
                    );
                }
            }
            if frame % 31 == 17 {
                let width = 480. + (frame % 3) as f32 * 10.;
                a.resize(width, 300.);
                b.resize(width, 300.);
            }
            if frame % 53 == 41 {
                let mut style = a.style(cards[1]);
                style.margin.top += 0.5;
                a.set_style(cards[1], style.clone());
                b.set_style(cards[1], style);
            }
            let (report_a, report_b) = (a.flush(), b.flush());
            assert_eq!(report_a.damage, report_b.damage, "damage at frame {frame}");
            assert_eq!(
                report_a.layout_nodes, report_b.layout_nodes,
                "layout at frame {frame}"
            );
            // What a renderer patches, each node with what changed (a box
            // alone, `BOX_ONLY`), the same.
            assert_eq!(a.repaints().1, b.repaints().1, "repaints at frame {frame}");
            for id in bars.iter().chain(&labels).chain(&cards).chain(&bodies) {
                assert_eq!(a.bounds(*id), b.bounds(*id), "frame {frame}");
            }
        }
        assert!(a.placing.is_empty());
        // Most frames placed their bars apart; the rest fell back.
        assert!(
            a.placed_parents > 250,
            "{} parents placed apart",
            a.placed_parents
        );
        assert_eq!(b.placed_parents, 0);
    }
}
