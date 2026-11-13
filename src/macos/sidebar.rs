use objc2::{MainThreadOnly, rc::Retained, sel};
use objc2_app_kit::{
    NSApplication, NSAutoresizingMaskOptions, NSBox, NSBoxType, NSButton, NSButtonType,
    NSCellImagePosition, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSFontWeightMedium, NSFontWeightSemibold, NSImage, NSImageScaling, NSImageView,
    NSLineBreakMode, NSTextAlignment, NSTextField, NSTitlePosition, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectView,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSString, ns_string};
use shuidi::{duration::format_minutes, model::Day};

use super::{APP_NAME, Delegate, rect};

pub(super) const WIDTH: f64 = 168.0;
const ROW_HEIGHT: f64 = 32.0;
const ROW_INSET: f64 = 12.0;
const TOP_ALIGNED: NSAutoresizingMaskOptions = NSAutoresizingMaskOptions::ViewMinYMargin;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Focus,
    Todos,
    History,
    Reward,
}

impl Page {
    const ALL: [Self; 4] = [Self::Focus, Self::Todos, Self::History, Self::Reward];

    pub(super) fn from_tag(tag: isize) -> Option<Self> {
        Self::ALL.get(usize::try_from(tag).ok()?).copied()
    }

    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Todos => "todos",
            Self::History => "history",
            Self::Reward => "reward",
        }
    }
}

pub(super) struct Sidebar {
    pub(super) view: Retained<NSVisualEffectView>,
    page: Page,
    buttons: [Retained<NSButton>; 4],
    selection: Retained<NSBox>,
    today: Retained<NSTextField>,
}

impl Sidebar {
    pub(super) fn new(target: &Delegate, height: f64) -> Self {
        let mtm = target.mtm();
        let view = NSVisualEffectView::initWithFrame(
            NSVisualEffectView::alloc(mtm),
            rect(0.0, 0.0, WIDTH, height),
        );
        view.setMaterial(NSVisualEffectMaterial::Sidebar);
        view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);

        let mark = NSImageView::initWithFrame(
            NSImageView::alloc(mtm),
            rect(18.0, height - 77.0, 22.0, 22.0),
        );
        let icon = NSApplication::sharedApplication(mtm).applicationIconImage();
        mark.setImage(icon.as_deref());
        mark.setImageScaling(NSImageScaling::ScaleProportionallyDown);
        mark.setAutoresizingMask(TOP_ALIGNED);
        view.addSubview(&mark);

        let title = label(
            mtm,
            &NSString::from_str(APP_NAME),
            rect(43.0, height - 77.0, WIDTH - 55.0, 22.0),
            &NSFont::systemFontOfSize_weight(15.0, unsafe { NSFontWeightSemibold }),
            &NSColor::labelColor(),
        );
        title.setAutoresizingMask(TOP_ALIGNED);
        view.addSubview(&title);

        let selection = NSBox::initWithFrame(
            NSBox::alloc(mtm),
            rect(
                ROW_INSET,
                height - 123.0,
                WIDTH - ROW_INSET * 2.0,
                ROW_HEIGHT,
            ),
        );
        selection.setBoxType(NSBoxType::Custom);
        selection.setTitlePosition(NSTitlePosition::NoTitle);
        selection.setBorderWidth(0.0);
        selection.setCornerRadius(8.0);
        selection.setFillColor(&NSColor::controlAccentColor().colorWithAlphaComponent(0.17));
        selection.setAutoresizingMask(TOP_ALIGNED);
        view.addSubview(&selection);

        let buttons = [
            (Page::Focus, ns_string!("专注"), ns_string!("timer")),
            (Page::Todos, ns_string!("待办"), ns_string!("checklist")),
            (Page::History, ns_string!("记录"), ns_string!("calendar")),
            (Page::Reward, ns_string!("奖励"), ns_string!("gift")),
        ]
        .map(|(page, title, symbol)| {
            let button = unsafe {
                NSButton::buttonWithTitle_target_action(
                    title,
                    Some(target),
                    Some(sel!(selectMainPage:)),
                    mtm,
                )
            };
            let icon = NSImage::imageWithSystemSymbolName_accessibilityDescription(symbol, None);
            button.setImage(icon.as_deref());
            button.setImagePosition(NSCellImagePosition::ImageLeft);
            button.setAlignment(NSTextAlignment::Left);
            button.setFont(Some(&NSFont::systemFontOfSize_weight(13.0, unsafe {
                NSFontWeightMedium
            })));
            button.setBordered(false);
            button.setButtonType(NSButtonType::PushOnPushOff);
            button.setTag(page as isize);
            button.setFrame(rect(
                22.0,
                height - 123.0 - page as u8 as f64 * 36.0,
                WIDTH - 42.0,
                ROW_HEIGHT,
            ));
            button.setAutoresizingMask(TOP_ALIGNED);
            view.addSubview(&button);
            button
        });

        let divider = NSBox::initWithFrame(NSBox::alloc(mtm), rect(16.0, 75.0, WIDTH - 32.0, 1.0));
        divider.setBoxType(NSBoxType::Separator);
        view.addSubview(&divider);

        let today_title = label(
            mtm,
            ns_string!("今日专注"),
            rect(21.0, 45.0, WIDTH - 40.0, 20.0),
            &NSFont::systemFontOfSize(12.0),
            &NSColor::secondaryLabelColor(),
        );
        view.addSubview(&today_title);
        let today = label(
            mtm,
            ns_string!("0 次   0分"),
            rect(21.0, 22.0, WIDTH - 40.0, 22.0),
            &NSFont::monospacedDigitSystemFontOfSize_weight(13.0, unsafe { NSFontWeightSemibold }),
            &NSColor::labelColor(),
        );
        today.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        view.addSubview(&today);

        let mut sidebar = Self {
            view,
            page: Page::Focus,
            buttons,
            selection,
            today,
        };
        sidebar.select(Page::Focus);
        sidebar
    }

    pub(super) fn page(&self) -> Page {
        self.page
    }

    pub(super) fn select(&mut self, page: Page) {
        self.page = page;
        let accent = NSColor::controlAccentColor();
        let text = NSColor::labelColor();
        for (item, button) in Page::ALL.into_iter().zip(&self.buttons) {
            if item == page {
                self.selection
                    .setFrameOrigin(NSPoint::new(ROW_INSET, button.frame().origin.y));
                button.setState(NSControlStateValueOn);
                button.setContentTintColor(Some(&accent));
            } else {
                button.setState(NSControlStateValueOff);
                button.setContentTintColor(Some(&text));
            }
        }
    }

    pub(super) fn set_today(&self, day: &Day) {
        self.today.setStringValue(&NSString::from_str(&format!(
            "{} 次   {}",
            day.count,
            format_minutes(day.minutes)
        )));
    }
}

fn label(
    mtm: MainThreadMarker,
    title: &NSString,
    frame: NSRect,
    font: &NSFont,
    color: &NSColor,
) -> Retained<NSTextField> {
    let view = NSTextField::labelWithString(title, mtm);
    view.setFrame(frame);
    view.setFont(Some(font));
    view.setTextColor(Some(color));
    view
}
