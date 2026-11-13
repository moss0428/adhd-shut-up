mod focus_sound;
mod notch;
mod sidebar;
mod status;

use std::{cell::RefCell, env, io, path::PathBuf, time::Instant};

use chrono::{Local, NaiveDate};
use focus_sound::FocusSound;
use notch::Notch;
use objc2::{
    AnyThread, DefinedClass, MainThreadOnly, define_class, msg_send,
    rc::{Retained, autoreleasepool},
    runtime::{AnyObject, NSObject, ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::{
    NSAboutPanelOptionCredits, NSAlert, NSApplication, NSApplicationActivationPolicy,
    NSApplicationDelegate, NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSColor,
    NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSMenu, NSMenuItem,
    NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSTextAlignment,
    NSTitlebarSeparatorStyle, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask,
    NSWindowTitleVisibility, NSWorkspace,
    NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{
    MainThreadMarker, NSAttributedString, NSDictionary, NSNotification, NSObjectProtocol, NSPoint,
    NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize, NSString, NSTimer, ns_string,
};
use objc2_web_kit::{
    WKScriptMessage, WKScriptMessageHandler, WKUserContentController, WKWebView,
    WKWebViewConfiguration, WKWebsiteDataStore,
};
use serde::Serialize;
use shuidi::{
    model::{Action, Clock, Model, Phase, Stamp, Summary},
    storage::Store,
};
use sidebar::{Page, Sidebar};
use status::StatusMenu;

use crate::APP_NAME;

const CHECKPOINT_MS: u64 = 30_000;
const MENU_FOCUS_MINUTES: u16 = 30;

pub(super) fn run() {
    let mtm = MainThreadMarker::new().expect("macOS must start on the main thread");
    autoreleasepool(|_| {
        let app = NSApplication::sharedApplication(mtm);
        let delegate = Delegate::new(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        app.run();
    });
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<Option<Application>>]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn launched(&self, _notification: &NSNotification) {
            let mtm = self.mtm();
            let app = NSApplication::sharedApplication(mtm);
            match Application::new(self, &app) {
                Ok(application) => {
                    *self.ivars().borrow_mut() = Some(application);
                    self.with_app(|application| {
                        application.load_page();
                        application.show_main();
                        application.sync(self);
                    });
                }
                Err(error) => {
                    let alert = NSAlert::new(mtm);
                    alert.setMessageText(ns_string!("无法打开本地记录"));
                    alert.setInformativeText(&NSString::from_str(&error.to_string()));
                    app.activate();
                    alert.runModal();
                    app.terminate(None);
                }
            }
        }

        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn reopened(&self, _app: &NSApplication, _has_visible_windows: bool) -> bool {
            self.present_main();
            true
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn keep_running(&self, _app: &NSApplication) -> bool {
            false
        }

        #[unsafe(method(applicationDidChangeScreenParameters:))]
        fn screen_changed(&self, _notification: &NSNotification) {
            self.with_app(|app| {
                app.notch.screen_changed(self.mtm());
                app.sync(self);
            });
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn terminate(&self, _notification: &NSNotification) {
            let mut state = self.ivars().borrow_mut();
            if let Some(mut app) = state.take() {
                app.shutdown(self);
            }
        }
    }

    unsafe impl WKScriptMessageHandler for Delegate {
        #[unsafe(method(userContentController:didReceiveScriptMessage:))]
        unsafe fn script_message(
            &self,
            _controller: &WKUserContentController,
            message: &WKScriptMessage,
        ) {
            // Only the bundled HTML can load content or post messages in this web view.
            let Ok(body) = (unsafe { message.body() }).downcast::<NSString>() else { return };
            let body = body.to_string();
            if body == "ready" {
                self.with_app(|app| {
                    app.web_ready = true;
                    app.send_to_page("shuidiShowPage", &app.sidebar.page().id());
                    app.page_dirty = true;
                    app.sync(self);
                });
            } else if body == "show_reward" {
                self.with_app(|app| app.select_page(Page::Reward));
            } else if body == "show_focus" {
                self.with_app(|app| app.select_page(Page::Focus));
            } else if let Ok(action) = serde_json::from_str::<Action>(&body) {
                self.command(action);
            } else {
                self.with_app(|app| {
                    app.set_notice(Some("无法读取这次操作，请检查输入后重试。".to_owned()));
                    app.page_dirty = true;
                    app.sync(self);
                });
            }
        }
    }

    impl Delegate {
        #[unsafe(method(pulse:))]
        fn pulse(&self, _timer: &NSTimer) {
            self.with_app(|app| app.tick(self));
        }

        #[unsafe(method(animate:))]
        fn animate(&self, _timer: &NSTimer) {
            self.with_app(|app| {
                let now_ms = app.now_ms();
                app.notch.paint_at(now_ms);
                app.status.animate(now_ms);
                if !app.notch.needs_motion(now_ms) && !app.status.needs_motion() {
                    app.stop_motion();
                }
            });
        }

        #[unsafe(method(systemWillSleep:))]
        fn sleep(&self, _notification: &NSNotification) {
            self.with_app(|app| {
                app.pause_for_sleep();
                app.sync(self);
            });
        }

        #[unsafe(method(displayAccessibilityChanged:))]
        fn accessibility_changed(&self, _notification: &NSNotification) {
            self.with_app(|app| {
                let reduce = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
                let now_ms = app.now_ms();
                app.notch.set_reduce_motion(reduce);
                app.notch.paint_at(now_ms);
                app.status.set_reduce_motion(reduce, now_ms);
                app.sync(self);
            });
        }

        #[unsafe(method(openMain:))]
        fn open_main(&self, _sender: Option<&AnyObject>) {
            self.present_main();
        }

        #[unsafe(method(showAbout:))]
        fn show_about(&self, _sender: Option<&AnyObject>) {
            let app = NSApplication::sharedApplication(self.mtm());
            let paragraph = NSMutableParagraphStyle::new();
            paragraph.setAlignment(NSTextAlignment::Center);
            let font = NSFont::systemFontOfSize(13.0);
            let color = NSColor::secondaryLabelColor();
            app.activate();
            // AppKit expects the credits option to contain an attributed string.
            unsafe {
                let attributes = NSDictionary::from_slices(
                    &[
                        NSParagraphStyleAttributeName,
                        NSFontAttributeName,
                        NSForegroundColorAttributeName,
                    ],
                    &[&*paragraph as &AnyObject, &*font, &*color],
                );
                let credits = NSAttributedString::initWithString_attributes(
                    NSAttributedString::alloc(),
                    ns_string!("这段时间我说了算。先做眼前这一件，ADHD，等会儿再吵。"),
                    Some(&attributes),
                );
                let options = NSDictionary::from_slices(
                    &[NSAboutPanelOptionCredits],
                    &[&*credits as &AnyObject],
                );
                app.orderFrontStandardAboutPanelWithOptions(&options);
            }
        }

        #[unsafe(method(closeMain:))]
        fn close_main(&self, _sender: Option<&AnyObject>) {
            self.with_app(|app| app.window.performClose(None));
        }

        #[unsafe(method(selectMainPage:))]
        fn select_main_page(&self, sender: &NSButton) {
            if let Some(page) = Page::from_tag(sender.tag()) {
                self.with_app(|app| app.select_page(page));
            }
        }

        #[unsafe(method(startFocus:))]
        fn start_focus(&self, _sender: Option<&AnyObject>) {
            self.with_app(|app| {
                let settings = app.model.settings();
                app.apply(Action::StartFocus {
                    minutes: MENU_FOCUS_MINUTES,
                    rest_minutes: settings.rest_minutes,
                });
                app.sync(self);
            });
        }

        #[unsafe(method(pauseTimer:))]
        fn pause_timer(&self, _sender: Option<&AnyObject>) {
            self.command(Action::Pause);
        }

        #[unsafe(method(resumeTimer:))]
        fn resume_timer(&self, _sender: Option<&AnyObject>) {
            self.command(Action::Resume);
        }

        #[unsafe(method(stopTimer:))]
        fn stop_timer(&self, _sender: Option<&AnyObject>) {
            self.command(Action::Stop);
        }

        #[unsafe(method(startRest:))]
        fn start_rest(&self, _sender: Option<&AnyObject>) {
            self.command(Action::StartRest);
        }

        #[unsafe(method(skipRest:))]
        fn skip_rest(&self, _sender: Option<&AnyObject>) {
            self.command(Action::SkipRest);
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        // NSObject initialization leaves the Rust-owned ivars in place.
        unsafe { msg_send![super(Self::alloc(mtm).set_ivars(RefCell::default())), init] }
    }

    fn with_app(&self, operation: impl FnOnce(&mut Application)) {
        // AppKit can deliver window notifications during a native window operation.
        let Ok(mut state) = self.ivars().try_borrow_mut() else {
            return;
        };
        if let Some(app) = state.as_mut() {
            operation(app);
        }
    }

    fn present_main(&self) {
        self.with_app(|app| {
            app.show_main();
            app.sync(self);
        });
    }

    fn command(&self, action: Action) {
        self.with_app(|app| {
            app.apply(action);
            app.sync(self);
        });
    }
}

struct Application {
    store: Store,
    model: Model,
    window: Retained<NSWindow>,
    sidebar: Sidebar,
    web: Retained<WKWebView>,
    notch: Notch,
    status: StatusMenu,
    focus_sound: FocusSound,
    origin: Instant,
    ticker: Retained<NSTimer>,
    motion: Option<Retained<NSTimer>>,
    next_checkpoint: u64,
    next_retry: u64,
    last_clock: Option<Clock>,
    last_day: Option<NaiveDate>,
    web_ready: bool,
    page_dirty: bool,
    notice: Option<String>,
}

#[derive(Serialize)]
struct PageState<'a> {
    #[serde(flatten)]
    summary: Summary,
    clock: Clock,
    notice: Option<&'a str>,
}

impl Application {
    fn new(target: &Delegate, app: &NSApplication) -> io::Result<Self> {
        let home =
            env::var_os("HOME").ok_or_else(|| io::Error::other("找不到当前用户的主目录。"))?;
        let store = Store::open(&PathBuf::from(home).join("Library/Application Support/水滴"))?;
        let model = Model::from_saved(store.load()?);

        let web = webview(target);
        let (window, sidebar) = main_window(&web, target);
        let mut notch = Notch::new(target);
        let workspace = NSWorkspace::sharedWorkspace();
        let reduce_motion = workspace.accessibilityDisplayShouldReduceMotion();
        notch.set_reduce_motion(reduce_motion);
        let center = workspace.notificationCenter();
        // NSWorkspace delivers these notification selectors to the app on the main thread.
        unsafe {
            center.addObserver_selector_name_object(
                target,
                sel!(systemWillSleep:),
                Some(NSWorkspaceWillSleepNotification),
                None,
            );
            center.addObserver_selector_name_object(
                target,
                sel!(displayAccessibilityChanged:),
                Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                None,
            );
        }

        let status = StatusMenu::new(target, app, reduce_motion);
        set_main_menu(target, app);
        let ticker = make_timer(target, 0.25, 0.055, sel!(pulse:));
        Ok(Self {
            store,
            model,
            window,
            sidebar,
            web,
            notch,
            status,
            focus_sound: FocusSound::default(),
            origin: Instant::now(),
            ticker,
            motion: None,
            next_checkpoint: CHECKPOINT_MS,
            next_retry: 0,
            last_clock: None,
            last_day: None,
            web_ready: false,
            page_dirty: true,
            notice: None,
        })
    }

    fn now_ms(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }

    fn stamp(&self) -> Stamp {
        Stamp::new(self.now_ms(), Local::now())
    }

    fn load_page(&self) {
        let html = include_str!("../ui/main.html")
            .replace("__APP_NAME__", APP_NAME)
            .replace("__STYLE__", include_str!("../ui/main.css"))
            .replace("__DEADLINE_SCRIPT__", include_str!("../ui/deadline.js"))
            .replace("__SCRIPT__", include_str!("../ui/main.js"));
        unsafe {
            self.web
                .loadHTMLString_baseURL(&NSString::from_str(&html), None)
        };
    }

    fn show_main(&mut self) {
        if self.window.isMiniaturized() {
            self.window.deminiaturize(None);
        }
        self.window.makeKeyAndOrderFront(None);
        NSApplication::sharedApplication(self.window.mtm()).activate();
        self.page_dirty = true;
    }

    fn select_page(&mut self, page: Page) {
        self.sidebar.select(page);
        if self.web_ready {
            self.send_to_page("shuidiShowPage", &page.id());
        }
    }

    fn tick(&mut self, target: &Delegate) {
        let now = self.stamp();
        if now.monotonic_ms >= self.next_retry {
            self.finish_due(now);
        }
        if self.model.is_running()
            && !self.model.is_due(now.monotonic_ms)
            && now.monotonic_ms >= self.next_checkpoint
        {
            self.checkpoint(now.monotonic_ms);
        }
        self.sync(target);
    }

    fn apply(&mut self, action: Action) {
        self.page_dirty = true;
        let now = self.stamp();
        if !self.finish_due(now) {
            return;
        }

        let mut next = self.model.clone();
        match next.apply(action, now) {
            Ok(()) => {
                self.commit(next, now.monotonic_ms);
            }
            Err(message) => self.set_notice(Some(message.to_owned())),
        }
    }

    fn finish_due(&mut self, now: Stamp) -> bool {
        if !self.model.is_due(now.monotonic_ms) {
            return true;
        }
        let mut next = self.model.clone();
        next.advance(now);
        self.commit(next, now.monotonic_ms)
    }

    fn commit(&mut self, next: Model, now_ms: u64) -> bool {
        match self.store.save(&next.saved_at(now_ms)) {
            Ok(()) => {
                self.model = next;
                self.next_checkpoint = now_ms + CHECKPOINT_MS;
                self.next_retry = 0;
                self.set_notice(None);
                self.page_dirty = true;
                true
            }
            Err(error) => {
                self.next_retry = now_ms + 2000;
                self.save_error(&error);
                false
            }
        }
    }

    fn checkpoint(&mut self, now_ms: u64) {
        self.next_checkpoint = now_ms + CHECKPOINT_MS;
        if let Err(error) = self.store.save(&self.model.saved_at(now_ms)) {
            self.save_error(&error);
        }
    }

    fn pause_for_sleep(&mut self) {
        let now = self.stamp();
        if self.finish_due(now) && self.model.is_running() {
            // Keep the in-memory timer paused even if the disk becomes unavailable.
            let _ = self.model.apply(Action::Pause, now);
            self.page_dirty = true;
            self.checkpoint(now.monotonic_ms);
        }
    }

    fn set_notice(&mut self, notice: Option<String>) {
        if self.notice != notice {
            self.notice = notice;
            self.page_dirty = true;
        }
    }

    fn save_error(&mut self, error: &io::Error) {
        eprintln!("{APP_NAME} 保存失败：{error}");
        self.set_notice(Some(
            "本地记录保存失败，请检查数据文件夹的写入权限或可用空间。".to_owned(),
        ));
    }

    fn sync(&mut self, target: &Delegate) {
        let now = self.stamp();
        let clock = self.model.clock(now.monotonic_ms);
        let settings = self.model.settings();
        if let Err(message) = self
            .focus_sound
            .set_track(settings.focus_audio.playing(clock))
        {
            self.set_notice(Some(message.to_owned()));
        }
        let rest_minutes = settings.rest_minutes;
        self.status.update(clock, rest_minutes, now.monotonic_ms);
        let menu_anchor = if clock.phase == Phase::Completed {
            self.status.anchor()
        } else {
            None
        };
        self.notch
            .update(clock, rest_minutes, now.monotonic_ms, menu_anchor);
        let motion = self.notch.needs_motion(now.monotonic_ms) || self.status.needs_motion();
        if motion && self.motion.is_none() {
            self.motion = Some(make_timer(target, 1.0 / 20.0, 0.006, sel!(animate:)));
        } else if !motion {
            self.stop_motion();
        }

        let clock_changed = self.last_clock != Some(clock);
        self.page_dirty |= self.last_day != Some(now.today())
            || self.last_clock.map(|previous| previous.phase) != Some(clock.phase);

        if self.web_ready && self.window.isVisible() && !self.window.isMiniaturized() {
            if self.page_dirty {
                let summary = self.model.summary(now.today());
                self.sidebar.set_today(&summary.today);
                let page = PageState {
                    summary,
                    clock,
                    notice: self.notice.as_deref(),
                };
                self.send_to_page("shuidiSetState", &page);
                self.page_dirty = false;
            } else if clock_changed {
                self.send_to_page("shuidiSetClock", &clock);
            }
        } else {
            self.page_dirty = true;
        }

        self.last_clock = Some(clock);
        self.last_day = Some(now.today());
    }

    fn send_to_page(&self, function: &str, value: &impl Serialize) {
        if let Ok(json) = serde_json::to_string(value) {
            let script = NSString::from_str(&format!("window.{function}({json})"));
            unsafe { self.web.evaluateJavaScript_completionHandler(&script, None) };
        }
    }

    fn stop_motion(&mut self) {
        if let Some(timer) = self.motion.take() {
            timer.invalidate();
        }
    }

    fn shutdown(&mut self, target: &Delegate) {
        self.pause_for_sleep();
        self.focus_sound.stop();
        self.ticker.invalidate();
        self.stop_motion();
        unsafe {
            NSWorkspace::sharedWorkspace()
                .notificationCenter()
                .removeObserver(target);
            self.web
                .configuration()
                .userContentController()
                .removeScriptMessageHandlerForName(ns_string!("shuidi"));
        }
    }
}

fn make_timer(target: &Delegate, seconds: f64, tolerance: f64, action: Sel) -> Retained<NSTimer> {
    // Each selector is implemented by Delegate and accepts the firing timer.
    unsafe {
        let timer = NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
            seconds, target, action, None, true,
        );
        timer.setTolerance(tolerance);
        NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
        timer
    }
}

fn webview(target: &Delegate) -> Retained<WKWebView> {
    let mtm = target.mtm();
    // The view loads only bundled HTML, with network access disabled by its CSP.
    unsafe {
        let configuration = WKWebViewConfiguration::new(mtm);
        configuration.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore(mtm));
        configuration
            .userContentController()
            .addScriptMessageHandler_name(ProtocolObject::from_ref(target), ns_string!("shuidi"));
        WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), NSRect::ZERO, &configuration)
    }
}

fn main_window(web: &WKWebView, target: &Delegate) -> (Retained<NSWindow>, Sidebar) {
    let mtm = target.mtm();
    let bounds = rect(0.0, 0.0, 774.0, 510.0);
    let window = unsafe {
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            bounds,
            NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Miniaturizable
                | NSWindowStyleMask::Resizable
                | NSWindowStyleMask::FullSizeContentView,
            NSBackingStoreType::Buffered,
            false,
        );
        // Closing keeps the timer and the window alive in Rust.
        window.setReleasedWhenClosed(false);
        window
    };
    window.setTitle(&NSString::from_str(APP_NAME));
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    window.setTitlebarAppearsTransparent(true);
    window.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::None);
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setContentMinSize(NSSize::new(590.0, 430.0));
    window.setCollectionBehavior(NSWindowCollectionBehavior::MoveToActiveSpace);

    let content = NSView::initWithFrame(NSView::alloc(mtm), bounds);
    let sidebar = Sidebar::new(target, bounds.size.height);
    web.setFrame(rect(
        sidebar::WIDTH,
        0.0,
        bounds.size.width - sidebar::WIDTH,
        bounds.size.height,
    ));
    web.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    content.addSubview(&sidebar.view);
    content.addSubview(web);
    window.setContentView(Some(&content));
    window.center();
    (window, sidebar)
}

fn set_main_menu(target: &Delegate, app: &NSApplication) {
    let mtm = target.mtm();
    let main = NSMenu::new(mtm);
    let app_item = menu_item(mtm, APP_NAME, None, "", None);
    let app_menu = NSMenu::new(mtm);
    app_menu.setTitle(&NSString::from_str(APP_NAME));
    app_menu.addItem(&menu_item(
        mtm,
        &format!("关于 {APP_NAME}"),
        Some(sel!(showAbout:)),
        "",
        Some(target),
    ));
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    app_menu.addItem(&menu_item(
        mtm,
        "打开主窗口",
        Some(sel!(openMain:)),
        "",
        Some(target),
    ));
    app_menu.addItem(&menu_item(
        mtm,
        "关闭窗口",
        Some(sel!(closeMain:)),
        "w",
        Some(target),
    ));
    app_menu.addItem(&NSMenuItem::separatorItem(mtm));
    app_menu.addItem(&menu_item(
        mtm,
        &format!("退出 {APP_NAME}"),
        Some(sel!(terminate:)),
        "q",
        Some(app),
    ));
    app_item.setSubmenu(Some(&app_menu));
    main.addItem(&app_item);

    let edit_item = menu_item(mtm, "编辑", None, "", None);
    let edit = NSMenu::new(mtm);
    edit.setTitle(ns_string!("编辑"));
    for (label, selector, key) in [
        ("撤销", sel!(undo:), "z"),
        ("剪切", sel!(cut:), "x"),
        ("复制", sel!(copy:), "c"),
        ("粘贴", sel!(paste:), "v"),
        ("全选", sel!(selectAll:), "a"),
    ] {
        edit.addItem(&menu_item(mtm, label, Some(selector), key, None));
    }
    edit_item.setSubmenu(Some(&edit));
    main.addItem(&edit_item);
    app.setMainMenu(Some(&main));
}

fn menu_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<Sel>,
    key: &str,
    target: Option<&AnyObject>,
) -> Retained<NSMenuItem> {
    // Selectors point to native first responder commands or the supplied target.
    unsafe {
        let item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key),
        );
        item.setTarget(target);
        item
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
}
