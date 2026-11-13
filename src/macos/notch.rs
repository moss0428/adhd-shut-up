use std::cell::Cell;

use objc2::{
    DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained, runtime::Sel, sel,
};
use objc2_app_kit::{
    NSBackingStoreType, NSBezelStyle, NSBezierPath, NSButton, NSColor, NSEvent, NSFont,
    NSFontWeightSemibold, NSPanel, NSScreen, NSStatusWindowLevel, NSTextField, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSRect, NSString, ns_string};
use shuidi::{
    duration::format_minutes,
    model::{Clock, Phase},
};

use super::{APP_NAME, Delegate, rect};

const PROMPT_WIDTH: f64 = 264.0;
const PROMPT_HEIGHT: f64 = 110.0;
const PROMPT_GAP: f64 = 6.0;
const PARTY_MS: u64 = 1500;

#[derive(Clone, Copy)]
struct Anchor {
    x: f64,
    top: f64,
}

impl Anchor {
    fn on_screen(mtm: MainThreadMarker) -> Option<Self> {
        let screens = NSScreen::screens(mtm);
        for screen in screens.iter() {
            let top_inset = screen.safeAreaInsets().top;
            let left = screen.auxiliaryTopLeftArea();
            let right = screen.auxiliaryTopRightArea();
            let left_edge = left.origin.x + left.size.width;
            let width = right.origin.x - left_edge;
            if top_inset > 0.0 && left.size.width > 0.0 && right.size.width > 0.0 && width > 0.0 {
                let frame = screen.frame();
                return Some(Self {
                    x: left_edge + width * 0.5,
                    top: frame.origin.y + frame.size.height - top_inset,
                });
            }
        }

        let screen = NSScreen::mainScreen(mtm).or_else(|| screens.firstObject())?;
        let frame = screen.visibleFrame();
        Some(Self {
            x: frame.origin.x + frame.size.width * 0.5,
            top: frame.origin.y + frame.size.height - 8.0,
        })
    }

    fn prompt_frame(self) -> NSRect {
        rect(
            (self.x - PROMPT_WIDTH * 0.5).round(),
            self.top - PROMPT_GAP - PROMPT_HEIGHT,
            PROMPT_WIDTH,
            PROMPT_HEIGHT,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct MenuAnchor {
    pub(super) button: NSRect,
    pub(super) screen: NSRect,
    pub(super) menu_bottom: f64,
}

impl MenuAnchor {
    fn prompt_frame(self) -> NSRect {
        let left = self.screen.origin.x + 8.0;
        let right = (self.screen.origin.x + self.screen.size.width - PROMPT_WIDTH - 8.0).max(left);
        let centered = self.button.origin.x + (self.button.size.width - PROMPT_WIDTH) * 0.5;
        rect(
            centered.clamp(left, right).round(),
            self.menu_bottom - PROMPT_GAP - PROMPT_HEIGHT,
            PROMPT_WIDTH,
            PROMPT_HEIGHT,
        )
    }
}

fn panel_placement(
    anchor: Option<Anchor>,
    menu: Option<MenuAnchor>,
    phase: Phase,
) -> Option<NSRect> {
    if phase != Phase::Completed {
        return None;
    }
    menu.map(MenuAnchor::prompt_frame)
        .or_else(|| anchor.map(Anchor::prompt_frame))
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Cell<f64>]
    struct NotchCanvas;

    impl NotchCanvas {
        #[unsafe(method(isFlipped))]
        fn flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw(&self, _dirty: NSRect) {
            draw_party(PROMPT_WIDTH - 55.0, self.ivars().get());
        }
    }
);

define_class!(
    #[unsafe(super(NSButton))]
    #[thread_kind = MainThreadOnly]
    struct NotchButton;

    impl NotchButton {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

impl NotchButton {
    fn new(target: &Delegate, title: &NSString, action: Sel) -> Retained<Self> {
        let button: Retained<Self> = unsafe {
            msg_send![super(Self::alloc(target.mtm()).set_ivars(())), initWithFrame: NSRect::ZERO]
        };
        button.setTitle(title);
        // The selectors take one sender object and are registered on Delegate.
        unsafe {
            button.setTarget(Some(target));
            button.setAction(Some(action));
        }
        button.setBezelStyle(NSBezelStyle::Push);
        button
    }
}

pub(super) struct Notch {
    panel: Retained<NSPanel>,
    canvas: Retained<NotchCanvas>,
    detail: Retained<NSTextField>,
    rest: Retained<NotchButton>,
    anchor: Option<Anchor>,
    menu_anchor: Option<MenuAnchor>,
    phase: Phase,
    phase_since_ms: u64,
    reduce_motion: bool,
}

impl Notch {
    pub(super) fn new(target: &Delegate) -> Self {
        let mtm = target.mtm();
        let bounds = rect(0.0, 0.0, PROMPT_WIDTH, PROMPT_HEIGHT);
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            bounds,
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        // Rust retains the panel for the whole application lifetime.
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setTitle(&NSString::from_str(&format!("{APP_NAME} 专注提示")));
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setHasShadow(true);
        panel.setFloatingPanel(true);
        panel.setHidesOnDeactivate(false);
        panel.setCanHide(false);
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setLevel(NSStatusWindowLevel);
        panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );

        let canvas: Retained<NotchCanvas> = unsafe {
            msg_send![
                super(NotchCanvas::alloc(mtm).set_ivars(Cell::default())),
                initWithFrame: bounds
            ]
        };
        canvas.setWantsLayer(true);
        let label = NSTextField::labelWithString(ns_string!("🎉  专注完成"), mtm);
        label.setTextColor(Some(&NSColor::labelColor()));
        label.setFont(Some(&NSFont::systemFontOfSize_weight(14.0, unsafe {
            NSFontWeightSemibold
        })));
        label.setFrame(rect(16.0, 14.0, PROMPT_WIDTH - 32.0, 22.0));

        let detail = NSTextField::labelWithString(ns_string!(""), mtm);
        detail.setTextColor(Some(&NSColor::secondaryLabelColor()));
        detail.setFont(Some(&NSFont::systemFontOfSize(12.0)));
        detail.setFrame(rect(16.0, 39.0, PROMPT_WIDTH - 32.0, 18.0));

        let material = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), bounds);
        material.setMaterial(NSVisualEffectMaterial::Popover);
        material.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        material.setState(NSVisualEffectState::Active);
        material.setWantsLayer(true);
        if let Some(layer) = material.layer() {
            layer.setCornerRadius(16.0);
            layer.setMasksToBounds(true);
        }

        let rest = NotchButton::new(target, ns_string!("休息"), sel!(startRest:));
        rest.setBezelColor(Some(&NSColor::controlAccentColor()));
        rest.setFrame(rect(PROMPT_WIDTH - 132.0, 64.0, 117.0, 34.0));
        let skip = NotchButton::new(target, ns_string!("先不休息"), sel!(skipRest:));
        skip.setFrame(rect(PROMPT_WIDTH - 226.0, 64.0, 88.0, 34.0));
        for view in [&*label as &NSView, &*detail, &*skip, &*rest] {
            canvas.addSubview(view);
        }
        let content = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        content.addSubview(&material);
        content.addSubview(&canvas);
        panel.setContentView(Some(&content));
        Self {
            panel,
            canvas,
            detail,
            rest,
            anchor: Anchor::on_screen(mtm),
            menu_anchor: None,
            phase: Phase::Idle,
            phase_since_ms: 0,
            reduce_motion: false,
        }
    }

    pub(super) fn screen_changed(&mut self, mtm: MainThreadMarker) {
        self.anchor = Anchor::on_screen(mtm);
        self.place();
    }

    pub(super) fn set_reduce_motion(&mut self, reduce: bool) {
        self.reduce_motion = reduce;
    }

    pub(super) fn update(
        &mut self,
        clock: Clock,
        rest_minutes: u16,
        now_ms: u64,
        menu_anchor: Option<MenuAnchor>,
    ) {
        let phase_changed = self.phase != clock.phase;
        let layout_changed = self.menu_anchor != menu_anchor;
        self.phase = clock.phase;
        self.menu_anchor = menu_anchor;

        if phase_changed && self.phase == Phase::Completed {
            self.phase_since_ms = now_ms;
            let completed = format_minutes(u64::from(clock.total_seconds / 60));
            let rest = format_minutes(u64::from(rest_minutes));
            self.detail
                .setStringValue(&NSString::from_str(&format!("已完成 {completed}")));
            self.rest
                .setTitle(&NSString::from_str(&format!("休息 {rest}")));
        }
        if phase_changed || layout_changed {
            self.place();
            self.paint_at(now_ms);
        }
    }

    fn place(&self) {
        if let Some(frame) = panel_placement(self.anchor, self.menu_anchor, self.phase) {
            self.panel.setFrame_display(frame, false);
            self.panel.invalidateShadow();
            self.panel.orderFrontRegardless();
        } else if self.panel.isVisible() {
            self.panel.orderOut(None);
        }
    }

    pub(super) fn needs_motion(&self, now_ms: u64) -> bool {
        self.panel.isVisible()
            && !self.reduce_motion
            && now_ms.saturating_sub(self.phase_since_ms) < PARTY_MS
    }

    pub(super) fn paint_at(&self, now_ms: u64) {
        if !self.panel.isVisible() {
            return;
        }
        let elapsed = if self.reduce_motion {
            2.0
        } else {
            now_ms.saturating_sub(self.phase_since_ms) as f64 / 1000.0
        };
        self.canvas.ivars().set(elapsed);
        self.canvas.setNeedsDisplay(true);
    }
}

fn color((r, g, b): (u8, u8, u8), alpha: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
        alpha,
    )
}

fn draw_party(x: f64, elapsed: f64) {
    let progress = (elapsed / 1.35).clamp(0.0, 1.0);
    let spread = 1.0 - (1.0 - progress).powi(3);
    let pieces = [
        (-33.0, -12.0, (163, 218, 194)),
        (-15.0, -20.0, (246, 192, 123)),
        (4.0, -15.0, (186, 205, 241)),
        (22.0, -18.0, (227, 150, 147)),
        (32.0, -3.0, (163, 218, 194)),
        (16.0, 12.0, (246, 192, 123)),
        (-5.0, 14.0, (227, 150, 147)),
        (-24.0, 7.0, (186, 205, 241)),
    ];
    for (index, (dx, dy, rgb)) in pieces.into_iter().enumerate() {
        let (width, height) = if index % 2 == 0 {
            (4.0, 5.5)
        } else {
            (5.5, 3.5)
        };
        let piece = rect(x + dx * spread, 35.0 + dy * spread, width, height);
        color(rgb, (progress * 5.0).min(0.87)).setFill();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(piece, 1.4, 1.4).fill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macbook() -> Anchor {
        Anchor {
            x: 855.0,
            top: 1074.5,
        }
    }

    #[test]
    fn menu_bar_countdown_does_not_show_a_second_window() {
        for phase in [
            Phase::Idle,
            Phase::Focus,
            Phase::FocusPaused,
            Phase::Rest,
            Phase::RestPaused,
        ] {
            assert_eq!(panel_placement(Some(macbook()), None, phase), None);
        }
    }

    #[test]
    fn completion_popup_follows_the_actual_menu_item_and_stays_on_screen() {
        let anchor = MenuAnchor {
            button: rect(1640.0, 1082.0, 62.0, 22.0),
            screen: rect(0.0, 0.0, 1710.0, 1112.0),
            menu_bottom: 1074.5,
        };
        assert_eq!(
            panel_placement(Some(macbook()), Some(anchor), Phase::Completed),
            Some(rect(1438.0, 958.5, 264.0, 110.0))
        );
    }

    #[test]
    fn completion_without_a_visible_menu_item_falls_back_below_the_cutout() {
        assert_eq!(
            panel_placement(Some(macbook()), None, Phase::Completed),
            Some(rect(723.0, 958.5, 264.0, 110.0))
        );
    }
}
