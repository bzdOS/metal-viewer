// START_AI_HEADER
// MODULE: mac-companion/metal-viewer/src/protocol.rs
// PURPOSE: Damage rect primitives — Rect struct, clamping, merging for partial surface updates.
// INTENT: Extracts damage rect logic from wayland_stream.rs into a testable lib module.
//         Used by Compositor to track per-surface damage regions.
// DEPENDENCIES: std (no external crates).
// PUBLIC_API: Rect, clamp_damage_to_surface, merge_damage.
// END_AI_HEADER

/// Axis-aligned rectangle (x, y, w, h) in pixels.
///
/// Used for damage regions in SURFACE_COMMIT events.
/// All coordinates are relative to the surface origin (top-left).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    // new:start
    //   purpose: Create a new rectangle.
    //   input:  x, y: top-left corner; w, h: dimensions.
    //   output: Rect instance.
    //   sideEffects: none (pure constructor).
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }
    // new:end

    // zero:start
    //   purpose: Create a zero-sized rectangle at (0, 0).
    //   input:  none.
    //   output: Rect with x=0, y=0, w=0, h=0.
    //   sideEffects: none (pure constructor).
    pub fn zero() -> Self {
        Self { x: 0, y: 0, w: 0, h: 0 }
    }
    // zero:end

    // is_zero:start
    //   purpose: Check if rectangle has zero area (w=0 or h=0).
    //   input:  &self.
    //   output: true if w=0 or h=0.
    //   sideEffects: none (pure query).
    pub fn is_zero(&self) -> bool {
        self.w == 0 || self.h == 0
    }
    // is_zero:end
}

// clamp_damage_to_surface:start
//   purpose: Clamp damage rect to surface bounds. If damage is zero-sized (w=0 or h=0),
//            treat as full surface (Wayland convention). If clamped rect has zero area,
//            return None (skip frame).
//   input:  damage: Rect from SURFACE_COMMIT; surface_w, surface_h: surface dimensions.
//   output: Some(clamped_rect) if valid, None if damage is completely outside bounds.
//   sideEffects: none (pure computation).
pub fn clamp_damage_to_surface(damage: Rect, surface_w: u16, surface_h: u16) -> Option<Rect> {
    // Zero-sized damage means full surface (Wayland protocol convention)
    if damage.is_zero() {
        return Some(Rect::new(0, 0, surface_w, surface_h));
    }

    // Clamp to surface bounds
    let x = damage.x.min(surface_w);
    let y = damage.y.min(surface_h);
    let x2 = (damage.x.saturating_add(damage.w)).min(surface_w);
    let y2 = (damage.y.saturating_add(damage.h)).min(surface_h);

    let w = x2.saturating_sub(x);
    let h = y2.saturating_sub(y);

    // If clamped rect has zero area, skip frame
    if w == 0 || h == 0 {
        None
    } else {
        Some(Rect::new(x, y, w, h))
    }
}
// clamp_damage_to_surface:end

// merge_damage:start
//   purpose: Merge two damage rects into a single bounding rect (union).
//   input:  d1, d2: two Rect instances.
//   output: Rect enclosing both input rects.
//   sideEffects: none (pure computation).
pub fn merge_damage(d1: Rect, d2: Rect) -> Rect {
    // If either is zero, return the other
    if d1.is_zero() {
        return d2;
    }
    if d2.is_zero() {
        return d1;
    }

    // Compute bounding box
    let x1 = d1.x.min(d2.x);
    let y1 = d1.y.min(d2.y);
    let x2 = (d1.x.saturating_add(d1.w)).max(d2.x.saturating_add(d2.w));
    let y2 = (d1.y.saturating_add(d1.h)).max(d2.y.saturating_add(d2.h));

    Rect::new(x1, y1, x2.saturating_sub(x1), y2.saturating_sub(y1))
}
// merge_damage:end

#[cfg(test)]
mod tests {
    use super::*;

    // ── clamp_damage_to_surface ─────────────────────────────────────────────

    #[test]
    fn damage_inside_bounds_passes_through() {
        let damage = Rect::new(10, 20, 30, 40);
        let result = clamp_damage_to_surface(damage, 100, 100);
        assert_eq!(result, Some(Rect::new(10, 20, 30, 40)));
    }

    #[test]
    fn damage_partially_outside_is_clamped() {
        let damage = Rect::new(80, 80, 50, 50);
        let result = clamp_damage_to_surface(damage, 100, 100);
        assert_eq!(result, Some(Rect::new(80, 80, 20, 20)));
    }

    #[test]
    fn damage_completely_outside_returns_none() {
        let damage = Rect::new(200, 200, 10, 10);
        let result = clamp_damage_to_surface(damage, 100, 100);
        assert_eq!(result, None);
    }

    #[test]
    fn damage_zero_size_means_full_surface() {
        let damage = Rect::new(0, 0, 0, 0);
        let result = clamp_damage_to_surface(damage, 100, 100);
        assert_eq!(result, Some(Rect::new(0, 0, 100, 100)));
    }

    #[test]
    fn damage_exactly_at_surface_edge_kept() {
        let damage = Rect::new(90, 90, 10, 10);
        let result = clamp_damage_to_surface(damage, 100, 100);
        assert_eq!(result, Some(Rect::new(90, 90, 10, 10)));
    }

    #[test]
    fn damage_with_overflow_xy_clamps() {
        let damage = Rect::new(65535, 65535, 100, 100);
        let result = clamp_damage_to_surface(damage, 100, 100);
        assert_eq!(result, None);
    }

    #[test]
    fn damage_zero_width_means_full_surface() {
        let damage = Rect::new(0, 0, 0, 50);
        let result = clamp_damage_to_surface(damage, 100, 100);
        assert_eq!(result, Some(Rect::new(0, 0, 100, 100)));
    }

    #[test]
    fn damage_zero_height_means_full_surface() {
        let damage = Rect::new(0, 0, 50, 0);
        let result = clamp_damage_to_surface(damage, 100, 100);
        assert_eq!(result, Some(Rect::new(0, 0, 100, 100)));
    }

    // ── merge_damage ────────────────────────────────────────────────────────

    #[test]
    fn merge_disjoint_rects_returns_enclosing() {
        let d1 = Rect::new(0, 0, 10, 10);
        let d2 = Rect::new(20, 20, 10, 10);
        let result = merge_damage(d1, d2);
        assert_eq!(result, Rect::new(0, 0, 30, 30));
    }

    #[test]
    fn merge_overlapping_rects_returns_enclosing() {
        let d1 = Rect::new(0, 0, 20, 20);
        let d2 = Rect::new(10, 10, 20, 20);
        let result = merge_damage(d1, d2);
        assert_eq!(result, Rect::new(0, 0, 30, 30));
    }

    #[test]
    fn merge_same_rects_returns_same() {
        let d1 = Rect::new(5, 5, 10, 10);
        let d2 = Rect::new(5, 5, 10, 10);
        let result = merge_damage(d1, d2);
        assert_eq!(result, Rect::new(5, 5, 10, 10));
    }

    #[test]
    fn merge_with_empty_returns_other() {
        let d1 = Rect::new(0, 0, 0, 0);
        let d2 = Rect::new(10, 10, 20, 20);
        let result = merge_damage(d1, d2);
        assert_eq!(result, Rect::new(10, 10, 20, 20));
    }
}
