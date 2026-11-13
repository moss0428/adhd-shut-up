use chrono::{NaiveDate, TimeZone, Utc};
use shuidi::{
    duration::{format_countdown, format_minutes},
    model::{Action, Model, Phase, SoundTrack, Stamp},
    storage::Store,
};

fn stamp(seconds: u64) -> Stamp {
    Stamp::new(
        seconds * 1000,
        Utc.with_ymd_and_hms(2026, 11, 11, 9, 0, 0).unwrap()
            + chrono::TimeDelta::seconds(seconds as i64),
    )
}

fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
}

fn start(model: &mut Model, minutes: u16, at: u64) {
    model
        .apply(
            Action::StartFocus {
                minutes,
                rest_minutes: 5,
            },
            stamp(at),
        )
        .unwrap();
}

#[test]
fn completion_is_credited_once_and_waits_for_a_rest_choice() {
    let mut model = Model::default();
    start(&mut model, 15, 0);
    assert!(!model.advance(stamp(899)));
    assert_eq!(model.clock(899_000).seconds, 1);
    assert!(model.advance(stamp(900)));
    assert!(!model.advance(stamp(940)));
    assert_eq!(model.clock(940_000).phase, Phase::Completed);
    let today = &model.summary(day(11)).today;
    assert_eq!((today.count, today.minutes), (1, 15));

    model.apply(Action::StartRest, stamp(940)).unwrap();
    assert_eq!(model.clock(940_000).phase, Phase::Rest);
    assert_eq!(model.clock(940_000).seconds, 300);
    assert!(model.advance(stamp(1240)));
    assert_eq!(model.clock(1_240_000).phase, Phase::Idle);
    assert_eq!(model.summary(day(11)).today.minutes, 15);
}

#[test]
fn pausing_and_reopening_does_not_count_time_away() {
    let mut model = Model::default();
    start(&mut model, 15, 0);
    model.apply(Action::Pause, stamp(300)).unwrap();
    assert!(!model.advance(stamp(1200)));
    assert_eq!(model.clock(1_200_000).seconds, 600);
    model.apply(Action::Resume, stamp(1200)).unwrap();

    let saved = model.saved_at(1_400_000);
    let mut reopened = Model::from_saved(saved);
    assert_eq!(reopened.clock(0).phase, Phase::FocusPaused);
    assert_eq!(reopened.clock(0).seconds, 400);
    reopened.apply(Action::Resume, stamp(10)).unwrap();
    assert!(!reopened.advance(stamp(409)));
    assert!(reopened.advance(stamp(410)));
    assert_eq!(reopened.summary(day(11)).today.minutes, 15);
}

#[test]
fn ending_early_adds_no_focus_time() {
    let mut model = Model::default();
    start(&mut model, 15, 0);
    model.apply(Action::Stop, stamp(300)).unwrap();
    assert_eq!(model.clock(300_000).phase, Phase::Idle);
    assert_eq!(model.summary(day(11)).today.count, 0);
}

#[test]
fn late_completion_uses_the_local_day_at_the_deadline() {
    let mut model = Model::default();
    let zone = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let before_midnight = zone.with_ymd_and_hms(2026, 11, 11, 23, 58, 0).unwrap();
    model
        .apply(
            Action::StartFocus {
                minutes: 1,
                rest_minutes: 5,
            },
            Stamp::new(0, before_midnight),
        )
        .unwrap();
    let late = before_midnight + chrono::TimeDelta::minutes(3);
    assert!(model.advance(Stamp::new(180_000, late)));
    let summary = model.summary(day(12));
    assert_eq!(summary.today.count, 0);
    assert_eq!(summary.days[1].date, day(11));
    assert_eq!(summary.days[1].minutes, 1);
}

#[test]
fn reward_counts_only_completed_sessions_since_it_was_set() {
    let mut model = Model::default();
    start(&mut model, 30, 0);
    assert!(model.advance(stamp(1800)));
    model.apply(Action::SkipRest, stamp(1800)).unwrap();

    model
        .apply(
            Action::SetGoal {
                name: "  一副耳机  ".into(),
                minutes: 45,
            },
            stamp(1800),
        )
        .unwrap();
    assert!(model.apply(Action::ClaimGoal, stamp(1800)).is_err());

    start(&mut model, 15, 1800);
    assert!(model.advance(stamp(2700)));
    model.apply(Action::SkipRest, stamp(2700)).unwrap();
    let goal = model.summary(day(11)).goal.unwrap();
    assert_eq!(goal.name, "一副耳机");
    assert_eq!(goal.earned_minutes, 15);
    assert!(!goal.reached);

    model
        .apply(
            Action::SetGoal {
                name: "耳机".into(),
                minutes: 30,
            },
            stamp(2700),
        )
        .unwrap();
    start(&mut model, 15, 2700);
    assert!(model.advance(stamp(3600)));
    let goal = model.summary(day(11)).goal.unwrap();
    assert_eq!(goal.earned_minutes, 30);
    assert!(goal.reached);

    model.apply(Action::ClaimGoal, stamp(3600)).unwrap();
    assert!(model.summary(day(11)).goal.unwrap().claimed);
    model
        .apply(
            Action::SetGoal {
                name: "一本书".into(),
                minutes: 60,
            },
            stamp(3600),
        )
        .unwrap();
    assert_eq!(model.summary(day(11)).goal.unwrap().earned_minutes, 0);
}

#[test]
fn saved_completion_survives_restart_without_duplication() {
    let directory = tempfile::tempdir().unwrap();
    let mut model = Model::default();
    start(&mut model, 1, 0);
    assert!(model.advance(stamp(60)));

    let store = Store::open(directory.path()).unwrap();
    store.save(&model.saved_at(60_000)).unwrap();
    assert!(Store::open(directory.path()).is_err());
    drop(store);

    let store = Store::open(directory.path()).unwrap();
    let mut reopened = Model::from_saved(store.load().unwrap());
    assert!(!reopened.advance(stamp(900)));
    assert_eq!(reopened.clock(900_000).phase, Phase::Completed);
    assert_eq!(reopened.summary(day(11)).today.count, 1);
}

#[test]
fn invalid_durations_cannot_start_a_session() {
    let mut model = Model::default();
    for (minutes, rest_minutes) in [(0, 5), (181, 5), (15, 0), (15, 61)] {
        assert!(
            model
                .apply(
                    Action::StartFocus {
                        minutes,
                        rest_minutes
                    },
                    stamp(0)
                )
                .is_err()
        );
        assert_eq!(model.clock(0).phase, Phase::Idle);
    }
}

#[test]
fn legacy_display_preferences_are_dropped_without_losing_timer_progress() {
    let mut model = Model::default();
    start(&mut model, 15, 0);
    let state = serde_json::to_value(model.saved_at(300_000)).unwrap();

    for position in ["left", "below", "right"] {
        let mut legacy = state.clone();
        legacy["settings"]["notch_position"] = serde_json::json!(position);
        let reopened = Model::from_saved(serde_json::from_value(legacy).unwrap());
        assert_eq!(reopened.settings().focus_minutes, 15);
        assert_eq!(reopened.settings().rest_minutes, 5);
        assert_eq!(reopened.clock(0).phase, Phase::FocusPaused);
        assert_eq!(reopened.clock(0).seconds, 600);
        let updated = serde_json::to_value(reopened.saved_at(0)).unwrap();
        assert!(updated["settings"].get("notch_position").is_none());
    }
}

#[test]
fn todo_focus_is_preserved_through_reopening_and_completion() {
    let due_at = day(12).and_hms_opt(15, 30, 0).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let mut model = Model::default();
    model
        .apply(
            Action::SaveTodo {
                id: None,
                name: "  写报告  ".into(),
                due_at: Some(due_at),
            },
            stamp(0),
        )
        .unwrap();
    let id = model.summary(day(11)).todos[0].todo.id;
    model
        .apply(
            Action::StartTodoFocus {
                todo_id: id,
                minutes: 15,
                rest_minutes: 5,
            },
            stamp(0),
        )
        .unwrap();
    store.save(&model.saved_at(300_000)).unwrap();

    let mut reopened = Model::from_saved(store.load().unwrap());
    let paused = reopened.clock(0);
    assert_eq!(
        (paused.phase, paused.seconds, paused.todo_id),
        (Phase::FocusPaused, 600, Some(id))
    );
    reopened.apply(Action::Resume, stamp(0)).unwrap();
    assert!(reopened.advance(stamp(600)));
    assert!(!reopened.advance(stamp(601)));
    reopened
        .apply(
            Action::SetTodoCompleted {
                id,
                completed: true,
            },
            stamp(601),
        )
        .unwrap();
    store.save(&reopened.saved_at(601_000)).unwrap();

    let summary = Model::from_saved(store.load().unwrap()).summary(day(11));
    let row = &summary.todos[0];
    assert_eq!(row.todo.name, "写报告");
    assert_eq!(row.todo.due_at, Some(due_at));
    assert!(row.todo.completed);
    assert_eq!((row.count, row.minutes), (1, 15));
    assert_eq!((summary.today.count, summary.today.minutes), (1, 15));
}

#[test]
fn todo_totals_follow_ids_and_include_only_completed_focus() {
    let mut model = Model::default();
    for _ in 0..2 {
        model
            .apply(
                Action::SaveTodo {
                    id: None,
                    name: "同名待办".into(),
                    due_at: None,
                },
                stamp(0),
            )
            .unwrap();
    }
    let summary = model.summary(day(11));
    let first = summary.todos[0].todo.id;
    let second = summary.todos[1].todo.id;
    assert_ne!(first, second);

    let mut at = 0;
    for (id, minutes) in [(first, 45), (second, 10), (first, 20)] {
        model
            .apply(
                Action::StartTodoFocus {
                    todo_id: id,
                    minutes,
                    rest_minutes: 5,
                },
                stamp(at),
            )
            .unwrap();
        at += u64::from(minutes) * 60;
        assert!(model.advance(stamp(at)));
        model.apply(Action::SkipRest, stamp(at)).unwrap();
    }
    model
        .apply(
            Action::StartTodoFocus {
                todo_id: second,
                minutes: 15,
                rest_minutes: 5,
            },
            stamp(at),
        )
        .unwrap();
    model.apply(Action::Stop, stamp(at + 60)).unwrap();
    start(&mut model, 5, at + 60);
    assert!(model.advance(stamp(at + 360)));
    model
        .apply(
            Action::SaveTodo {
                id: Some(first),
                name: "改名后".into(),
                due_at: None,
            },
            stamp(at + 360),
        )
        .unwrap();

    let summary = model.summary(day(11));
    assert_eq!(summary.todos[0].todo.name, "改名后");
    assert_eq!((summary.todos[0].count, summary.todos[0].minutes), (2, 65));
    assert_eq!((summary.todos[1].count, summary.todos[1].minutes), (1, 10));
    assert_eq!(
        (summary.unassigned.count, summary.unassigned.minutes),
        (1, 5)
    );
    assert_eq!((summary.today.count, summary.today.minutes), (4, 80));
}

#[test]
fn legacy_focus_data_loads_as_unassigned() {
    let saved = serde_json::from_str(
        r#"{
        "settings": {"focus_minutes": 30, "rest_minutes": 5},
        "sessions": [{"date": "2026-11-11", "minutes": 15}],
        "goal": null,
        "stage": {"phase": "focus", "timer": {"minutes": 30, "remaining_ms": 600000}}
    }"#,
    )
    .unwrap();
    let model = Model::from_saved(saved);
    assert_eq!(model.clock(0).phase, Phase::FocusPaused);
    assert_eq!(model.clock(0).todo_id, None);
    assert!(!model.settings().focus_audio.enabled);
    let summary = model.summary(day(11));
    assert!(summary.todos.is_empty());
    assert_eq!(
        (summary.unassigned.count, summary.unassigned.minutes),
        (1, 15)
    );
}

#[test]
fn focus_audio_keeps_the_chosen_track_and_plays_only_during_focus() {
    let mut model = Model::default();
    model
        .apply(
            Action::SetFocusAudio {
                enabled: true,
                track: SoundTrack::CoffeeShop,
            },
            stamp(0),
        )
        .unwrap();
    assert_eq!(model.settings().focus_audio.playing(model.clock(0)), None);

    start(&mut model, 1, 0);
    assert_eq!(
        model.settings().focus_audio.playing(model.clock(1_000)),
        Some(SoundTrack::CoffeeShop)
    );
    model.apply(Action::Pause, stamp(10)).unwrap();
    assert_eq!(
        model.settings().focus_audio.playing(model.clock(10_000)),
        None
    );

    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    store.save(&model.saved_at(10_000)).unwrap();
    let mut reopened = Model::from_saved(store.load().unwrap());
    reopened.apply(Action::Resume, stamp(0)).unwrap();
    assert_eq!(
        reopened.settings().focus_audio.playing(reopened.clock(0)),
        Some(SoundTrack::CoffeeShop)
    );
    reopened
        .apply(
            Action::SetFocusAudio {
                enabled: false,
                track: SoundTrack::CoffeeShop,
            },
            stamp(1),
        )
        .unwrap();
    assert_eq!(
        reopened
            .settings()
            .focus_audio
            .playing(reopened.clock(1_000)),
        None
    );
    reopened
        .apply(
            Action::SetFocusAudio {
                enabled: true,
                track: SoundTrack::LightRain,
            },
            stamp(2),
        )
        .unwrap();
    assert_eq!(
        reopened
            .settings()
            .focus_audio
            .playing(reopened.clock(2_000)),
        Some(SoundTrack::LightRain)
    );
    assert_eq!(
        reopened
            .settings()
            .focus_audio
            .playing(reopened.clock(50_000)),
        None
    );
    assert!(reopened.advance(stamp(50)));
    reopened.apply(Action::StartRest, stamp(50)).unwrap();
    assert_eq!(
        reopened
            .settings()
            .focus_audio
            .playing(reopened.clock(51_000)),
        None
    );
}

#[test]
fn durations_roll_over_at_the_hour_and_day_boundaries() {
    for (minutes, display) in [
        (0, "0分"),
        (59, "59分"),
        (60, "1小时"),
        (65, "1小时5分"),
        (1439, "23小时59分"),
        (1440, "1天"),
        (1565, "1天2小时5分"),
    ] {
        assert_eq!(format_minutes(minutes), display);
    }
    assert_eq!(format_countdown(3599), "59分59秒");
    assert_eq!(format_countdown(3630), "1小时00分30秒");
    assert_eq!(format_countdown(86_401), "1天0小时00分01秒");
}
