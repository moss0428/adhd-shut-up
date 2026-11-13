use std::f64::consts::TAU;

use block2::RcBlock;
use objc2::{AnyThread, MainThreadOnly, rc::Retained, runtime::AnyObject, sel};
use objc2_app_kit::{
    NSAccessibility, NSApplication, NSCellImagePosition, NSCompositingOperation, NSFont,
    NSFontAttributeName, NSFontWeightMedium, NSImage, NSMenu, NSMenuItem, NSStatusBar,
    NSStatusBarButton, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{
    NSAttributedString, NSData, NSDictionary, NSPoint, NSRect, NSSize, NSString, ns_string,
};
use shuidi::{
    duration::{format_countdown, format_minutes},
    model::{Clock, Phase},
};

use super::notch::MenuAnchor;
use super::{APP_NAME, Delegate, MENU_FOCUS_MINUTES, menu_item};

const ICON_FRAME_MS: u64 = 100;

pub(super) struct StatusMenu {
    item: Retained<NSStatusItem>,
    button: Option<Retained<NSStatusBarButton>>,
    label: Retained<NSMenuItem>,
    start: Retained<NSMenuItem>,
    pause: Retained<NSMenuItem>,
    resume: Retained<NSMenuItem>,
    stop: Retained<NSMenuItem>,
    rest: Retained<NSMenuItem>,
    skip: Retained<NSMenuItem>,
    kick_icon: Retained<NSImage>,
    rest_icon: Retained<NSImage>,
    completion_icon: Retained<NSImage>,
    clock: Option<Clock>,
    icon_phase: Phase,
    motion_started: u64,
    motion_frame: Option<u64>,
    reduce_motion: bool,
}

impl StatusMenu {
    pub(super) fn new(target: &Delegate, app: &NSApplication, reduce_motion: bool) -> Self {
        let mtm = target.mtm();
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        item.setAutosaveName(Some(ns_string!("WaterDropStatus")));
        let button = item.button(mtm);
        let kick_icon = load_kick_icon();
        if let Some(button) = &button {
            button.setImage(Some(&kick_icon));
            button.setToolTip(Some(&NSString::from_str(APP_NAME)));
        }

        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let label = menu_item(mtm, APP_NAME, None, "", None);
        label.setEnabled(false);
        menu.addItem(&label);
        let duration = format_minutes(u64::from(MENU_FOCUS_MINUTES));
        let start = menu_item(
            mtm,
            &format!("开始专注 {duration}"),
            Some(sel!(startFocus:)),
            "",
            Some(target),
        );
        let pause = menu_item(mtm, "暂停", Some(sel!(pauseTimer:)), "", Some(target));
        let resume = menu_item(mtm, "继续", Some(sel!(resumeTimer:)), "", Some(target));
        let stop = menu_item(mtm, "停止", Some(sel!(stopTimer:)), "", Some(target));
        let rest = menu_item(mtm, "开始休息", Some(sel!(startRest:)), "", Some(target));
        let skip = menu_item(mtm, "先不休息", Some(sel!(skipRest:)), "", Some(target));
        menu.addItem(&start);
        for action in [&pause, &resume, &stop, &rest, &skip] {
            action.setHidden(true);
            menu.addItem(action);
        }
        menu.addItem(&menu_item(
            mtm,
            "打开主窗口",
            Some(sel!(openMain:)),
            "",
            Some(target),
        ));
        menu.addItem(&menu_item(
            mtm,
            &format!("关于 {APP_NAME}"),
            Some(sel!(showAbout:)),
            "",
            Some(target),
        ));
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        menu.addItem(&menu_item(
            mtm,
            &format!("退出 {APP_NAME}"),
            Some(sel!(terminate:)),
            "",
            Some(app),
        ));
        item.setMenu(Some(&menu));
        Self {
            item,
            button,
            label,
            start,
            pause,
            resume,
            stop,
            rest,
            skip,
            kick_icon,
            rest_icon: load_menu_icon(
                include_bytes!("../../assets/rest-menu-recline.png"),
                NSSize::new(28.0, 18.0),
            ),
            completion_icon: load_menu_icon(
                include_bytes!("../../assets/completion-menu-peace.png"),
                NSSize::new(28.0, 18.0),
            ),
            clock: None,
            icon_phase: Phase::Idle,
            motion_started: 0,
            motion_frame: None,
            reduce_motion,
        }
    }

    pub(super) fn update(&mut self, clock: Clock, rest_minutes: u16, now_ms: u64) {
        let phase = clock.phase;
        let phase_changed = self.icon_phase != phase;
        if self.clock == Some(clock) && !phase_changed {
            return;
        }
        self.clock = Some(clock);

        let timer = format_countdown(clock.seconds);
        let label = match clock.phase {
            Phase::Idle => APP_NAME.to_owned(),
            Phase::Focus => format!("专注中  {timer}"),
            Phase::FocusPaused => format!("专注已暂停  {timer}"),
            Phase::Completed => "专注完成".to_owned(),
            Phase::Rest => format!("休息中  {timer}"),
            Phase::RestPaused => format!("休息已暂停  {timer}"),
        };
        self.label.setTitle(&NSString::from_str(&label));
        if phase == Phase::Completed {
            let duration = format_minutes(u64::from(rest_minutes));
            self.rest
                .setTitle(&NSString::from_str(&format!("休息 {duration}")));
        }

        if phase_changed {
            let running = matches!(phase, Phase::Focus | Phase::Rest);
            let paused = matches!(phase, Phase::FocusPaused | Phase::RestPaused);
            self.start.setHidden(phase != Phase::Idle);
            self.pause.setHidden(!running);
            self.resume.setHidden(!paused);
            self.stop.setHidden(!(running || paused));
            self.rest.setHidden(phase != Phase::Completed);
            self.skip.setHidden(phase != Phase::Completed);

            self.icon_phase = phase;
            self.motion_started = now_ms;
            self.motion_frame = None;
            self.set_icon(now_ms);
        }
        let Some(button) = &self.button else { return };
        match phase {
            Phase::Idle | Phase::Completed => {
                button.setTitle(ns_string!(""));
                button.setImagePosition(NSCellImagePosition::ImageOnly);
            }
            _ => {
                button.setAttributedTitle(&timer_title(&timer));
                button.setImagePosition(NSCellImagePosition::ImageLeft);
            }
        }
        let accessible = if clock.phase == Phase::Idle {
            label
        } else {
            format!("{APP_NAME} {label}")
        };
        button.setAccessibilityLabel(Some(&NSString::from_str(&accessible)));
    }

    pub(super) fn anchor(&self) -> Option<MenuAnchor> {
        let button = self.button.as_ref()?;
        let window = button.window()?;
        let display = window.screen()?;
        let screen = display.frame();
        let rect = window.convertRectToScreen(button.convertRect_toView(button.bounds(), None));
        if !self.item.isVisible()
            || !window.isVisible()
            || button.isHiddenOrHasHiddenAncestor()
            || rect.size.width <= 0.0
            || rect.size.height <= 0.0
            || rect.origin.y < screen.origin.y
            || rect.origin.y >= screen.origin.y + screen.size.height
        {
            return None;
        }
        let visible = display.visibleFrame();
        let safe_top = screen.origin.y + screen.size.height - display.safeAreaInsets().top;
        let menu_bottom = rect
            .origin
            .y
            .min(visible.origin.y + visible.size.height)
            .min(safe_top);
        Some(MenuAnchor {
            button: rect,
            screen,
            menu_bottom,
        })
    }

    pub(super) fn set_reduce_motion(&mut self, reduce: bool, now_ms: u64) {
        self.reduce_motion = reduce;
        self.motion_frame = None;
        self.animate(now_ms);
    }

    pub(super) fn needs_motion(&self) -> bool {
        !self.reduce_motion
            && matches!(self.icon_phase, Phase::Focus | Phase::Rest)
            && self.item.isVisible()
    }

    pub(super) fn animate(&mut self, now_ms: u64) {
        if matches!(self.icon_phase, Phase::Focus | Phase::Rest) {
            self.set_icon(now_ms);
        }
    }

    fn set_icon(&mut self, now_ms: u64) {
        let Some(button) = &self.button else { return };
        let icon = match self.icon_phase {
            Phase::Idle => Some(self.kick_icon.clone()),
            Phase::FocusPaused | Phase::RestPaused => symbol(ns_string!("pause.fill")),
            Phase::Completed => Some(self.completion_icon.clone()),
            phase @ (Phase::Focus | Phase::Rest) => {
                let frame = if self.reduce_motion {
                    0
                } else {
                    now_ms.saturating_sub(self.motion_started) / ICON_FRAME_MS
                };
                if self.motion_frame == Some(frame) {
                    return;
                }
                self.motion_frame = Some(frame);
                let image = if phase == Phase::Focus {
                    &self.kick_icon
                } else {
                    &self.rest_icon
                };
                Some(animated_icon(image, frame))
            }
        };
        button.setImage(icon.as_deref());
    }
}

fn symbol(name: &NSString) -> Option<Retained<NSImage>> {
    let icon = NSImage::imageWithSystemSymbolName_accessibilityDescription(name, None)?;
    icon.setTemplate(true);
    Some(icon)
}

fn load_kick_icon() -> Retained<NSImage> {
    load_menu_icon(
        include_bytes!("../../assets/focus-menu-kick.png"),
        NSSize::new(48.0, 18.0),
    )
}

fn load_menu_icon(bytes: &[u8], size: NSSize) -> Retained<NSImage> {
    let data = NSData::with_bytes(bytes);
    let image =
        NSImage::initWithData(NSImage::alloc(), &data).expect("invalid bundled menu bar icon");
    image.setSize(size);
    image.setTemplate(true);
    image
}

fn animated_icon(image: &Retained<NSImage>, frame: u64) -> Retained<NSImage> {
    let elapsed = frame as f64 * ICON_FRAME_MS as f64 / 1000.0;
    let opacity = 0.96 + 0.04 * (elapsed * TAU / 6.8).cos();
    let size = image.size();
    let image = image.clone();
    let draw = RcBlock::new(move |_area: NSRect| {
        image.drawInRect_fromRect_operation_fraction(
            NSRect::new(NSPoint::ZERO, size),
            NSRect::ZERO,
            NSCompositingOperation::SourceOver,
            opacity,
        );
        true.into()
    });
    let icon = NSImage::imageWithSize_flipped_drawingHandler(size, false, &draw);
    icon.setTemplate(true);
    icon
}

fn timer_title(time: &str) -> Retained<NSAttributedString> {
    let font = NSFont::monospacedDigitSystemFontOfSize_weight(13.0, unsafe { NSFontWeightMedium });
    let attrs =
        NSDictionary::from_slices(&[unsafe { NSFontAttributeName }], &[&*font as &AnyObject]);
    // The status bar supplies the text color for the current menu bar appearance.
    unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(time),
            Some(&attrs),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_menu_icon_uses_the_preview_size() {
        let image = animated_icon(&load_kick_icon(), 0);
        assert_eq!(image.size(), NSSize::new(48.0, 18.0));
        assert!(image.isTemplate());
    }
}
