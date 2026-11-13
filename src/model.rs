use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeDelta, TimeZone};
use serde::{Deserialize, Serialize};

const MINUTE_MS: u64 = 60_000;

#[derive(Clone, Copy)]
pub struct Stamp {
    pub monotonic_ms: u64,
    local: DateTime<FixedOffset>,
}

impl Stamp {
    pub fn new<T: TimeZone>(monotonic_ms: u64, local: DateTime<T>) -> Self {
        Self {
            monotonic_ms,
            local: local.fixed_offset(),
        }
    }

    pub fn today(self) -> NaiveDate {
        self.local.date_naive()
    }

    fn date_at(self, monotonic_ms: u64) -> NaiveDate {
        let elapsed = self.monotonic_ms.saturating_sub(monotonic_ms);
        i64::try_from(elapsed)
            .ok()
            .and_then(TimeDelta::try_milliseconds)
            .and_then(|delta| self.local.checked_sub_signed(delta))
            .map_or_else(|| self.today(), |then| then.date_naive())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Settings {
    pub focus_minutes: u16,
    pub rest_minutes: u16,
    #[serde(default)]
    pub focus_audio: FocusAudio,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            focus_minutes: 15,
            rest_minutes: 5,
            focus_audio: FocusAudio::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct FocusAudio {
    pub enabled: bool,
    pub track: SoundTrack,
}

impl FocusAudio {
    pub fn playing(self, clock: Clock) -> Option<SoundTrack> {
        (self.enabled && clock.phase == Phase::Focus && clock.seconds > 0).then_some(self.track)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundTrack {
    #[default]
    LightRain,
    CoffeeShop,
    Library,
    Fireplace,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Session {
    date: NaiveDate,
    minutes: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    todo_id: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Todo {
    pub id: u32,
    pub name: String,
    pub due_at: Option<NaiveDateTime>,
    pub completed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Goal {
    name: String,
    minutes: u32,
    first_session: usize,
    claimed_minutes: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct Countdown {
    minutes: u16,
    remaining_ms: u64,
}

impl Countdown {
    fn new(minutes: u16) -> Self {
        Self {
            minutes,
            remaining_ms: u64::from(minutes) * MINUTE_MS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
enum Stage {
    #[default]
    Idle,
    Focus {
        timer: Countdown,
        #[serde(skip_serializing_if = "Option::is_none")]
        todo_id: Option<u32>,
    },
    Completed {
        minutes: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        todo_id: Option<u32>,
    },
    Rest {
        timer: Countdown,
    },
}

impl Stage {
    fn timer_mut(&mut self) -> Option<&mut Countdown> {
        match self {
            Self::Focus { timer, .. } | Self::Rest { timer } => Some(timer),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Saved {
    settings: Settings,
    sessions: Vec<Session>,
    goal: Option<Goal>,
    stage: Stage,
    #[serde(default)]
    todos: Vec<Todo>,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    saved: Saved,
    deadline_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    StartFocus {
        minutes: u16,
        rest_minutes: u16,
    },
    StartTodoFocus {
        todo_id: u32,
        minutes: u16,
        rest_minutes: u16,
    },
    Pause,
    Resume,
    Stop,
    StartRest,
    SkipRest,
    SetFocusAudio {
        enabled: bool,
        track: SoundTrack,
    },
    SaveTodo {
        id: Option<u32>,
        name: String,
        due_at: Option<NaiveDateTime>,
    },
    SetTodoCompleted {
        id: u32,
        completed: bool,
    },
    SetGoal {
        name: String,
        minutes: u32,
    },
    ClaimGoal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    Focus,
    FocusPaused,
    Completed,
    Rest,
    RestPaused,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Clock {
    pub phase: Phase,
    pub seconds: u32,
    pub total_seconds: u32,
    pub todo_id: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    pub count: usize,
    pub minutes: u64,
}

#[derive(Debug, Serialize)]
pub struct GoalProgress {
    pub name: String,
    pub target_minutes: u32,
    pub earned_minutes: u64,
    pub reached: bool,
    pub claimed: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct FocusTotals {
    pub count: usize,
    pub minutes: u64,
}

#[derive(Debug, Serialize)]
pub struct TodoSummary {
    #[serde(flatten)]
    pub todo: Todo,
    pub count: usize,
    pub minutes: u64,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub settings: Settings,
    pub today: Day,
    pub days: Vec<Day>,
    pub goal: Option<GoalProgress>,
    pub todos: Vec<TodoSummary>,
    pub unassigned: FocusTotals,
}

impl Model {
    pub fn from_saved(saved: Saved) -> Self {
        Self {
            saved,
            deadline_ms: None,
        }
    }

    pub fn saved_at(&self, now_ms: u64) -> Saved {
        let mut saved = self.saved.clone();
        if let (Some(deadline), Some(timer)) = (self.deadline_ms, saved.stage.timer_mut()) {
            timer.remaining_ms = deadline.saturating_sub(now_ms);
        }
        saved
    }

    pub fn is_running(&self) -> bool {
        self.deadline_ms.is_some()
    }

    pub fn is_due(&self, now_ms: u64) -> bool {
        self.deadline_ms.is_some_and(|deadline| now_ms >= deadline)
    }

    pub fn settings(&self) -> Settings {
        self.saved.settings
    }

    pub fn advance(&mut self, now: Stamp) -> bool {
        let Some(deadline) = self.deadline_ms else {
            return false;
        };
        if now.monotonic_ms < deadline {
            return false;
        }

        self.deadline_ms = None;
        self.saved.stage = match self.saved.stage {
            Stage::Focus { timer, todo_id } => {
                self.saved.sessions.push(Session {
                    date: now.date_at(deadline),
                    minutes: timer.minutes,
                    todo_id,
                });
                Stage::Completed {
                    minutes: timer.minutes,
                    todo_id,
                }
            }
            _ => Stage::Idle,
        };
        true
    }

    pub fn apply(&mut self, action: Action, now: Stamp) -> Result<(), &'static str> {
        let now_ms = now.monotonic_ms;
        match action {
            Action::StartFocus {
                minutes,
                rest_minutes,
            } => self.start_focus(minutes, rest_minutes, None, now_ms)?,
            Action::StartTodoFocus {
                todo_id,
                minutes,
                rest_minutes,
            } => {
                if !self
                    .saved
                    .todos
                    .iter()
                    .any(|todo| todo.id == todo_id && !todo.completed)
                {
                    return Err("请选择未完成的待办。");
                }
                self.start_focus(minutes, rest_minutes, Some(todo_id), now_ms)?;
            }
            Action::Pause if self.is_running() => {
                let deadline = self.deadline_ms.take().unwrap();
                if let Some(timer) = self.saved.stage.timer_mut() {
                    timer.remaining_ms = deadline.saturating_sub(now_ms);
                }
            }
            Action::Resume if !self.is_running() => {
                let timer = self.saved.stage.timer_mut().ok_or("当前没有暂停的计时。")?;
                self.deadline_ms = Some(now_ms.saturating_add(timer.remaining_ms));
            }
            Action::Stop if self.saved.stage.timer_mut().is_some() => {
                self.deadline_ms = None;
                self.saved.stage = Stage::Idle;
            }
            Action::StartRest if matches!(self.saved.stage, Stage::Completed { .. }) => {
                let timer = Countdown::new(self.saved.settings.rest_minutes);
                self.saved.stage = Stage::Rest { timer };
                self.deadline_ms = Some(now_ms.saturating_add(timer.remaining_ms));
            }
            Action::SkipRest if matches!(self.saved.stage, Stage::Completed { .. }) => {
                self.saved.stage = Stage::Idle;
            }
            Action::SetFocusAudio { enabled, track } => {
                self.saved.settings.focus_audio = FocusAudio { enabled, track };
            }
            Action::SaveTodo { id, name, due_at } => self.save_todo(id, name, due_at)?,
            Action::SetTodoCompleted { id, completed } => {
                self.todo_mut(id)?.completed = completed;
            }
            Action::SetGoal { name, minutes } => self.set_goal(name, minutes)?,
            Action::ClaimGoal => {
                let goal = self.goal_progress().ok_or("请先设定奖励目标。")?;
                if !goal.reached || goal.claimed {
                    return Err("达成目标后可以领取一次奖励。");
                }
                if let Some(saved_goal) = &mut self.saved.goal {
                    saved_goal.claimed_minutes = Some(goal.earned_minutes);
                }
            }
            _ => return Err("计时状态已更新，请再试一次。"),
        }
        Ok(())
    }

    pub fn clock(&self, now_ms: u64) -> Clock {
        let (phase, timer, todo_id) = match self.saved.stage {
            Stage::Focus { timer, todo_id } if self.is_running() => (Phase::Focus, timer, todo_id),
            Stage::Focus { timer, todo_id } => (Phase::FocusPaused, timer, todo_id),
            Stage::Rest { timer } if self.is_running() => (Phase::Rest, timer, None),
            Stage::Rest { timer } => (Phase::RestPaused, timer, None),
            Stage::Idle => {
                let seconds = u32::from(self.saved.settings.focus_minutes) * 60;
                return Clock {
                    phase: Phase::Idle,
                    seconds,
                    total_seconds: seconds,
                    todo_id: None,
                };
            }
            Stage::Completed { minutes, todo_id } => {
                return Clock {
                    phase: Phase::Completed,
                    seconds: 0,
                    total_seconds: u32::from(minutes) * 60,
                    todo_id,
                };
            }
        };
        let remaining_ms = self.deadline_ms.map_or(timer.remaining_ms, |deadline| {
            deadline.saturating_sub(now_ms)
        });
        let total_seconds = u32::from(timer.minutes) * 60;
        Clock {
            phase,
            seconds: remaining_ms.div_ceil(1000).min(u64::from(total_seconds)) as u32,
            total_seconds,
            todo_id,
        }
    }

    pub fn summary(&self, today: NaiveDate) -> Summary {
        let mut days = BTreeMap::new();
        for days_ago in 0..7 {
            if let Some(date) = today.checked_sub_signed(TimeDelta::days(days_ago)) {
                days.insert(
                    date,
                    Day {
                        date,
                        count: 0,
                        minutes: 0,
                    },
                );
            }
        }
        let mut focus_by_todo = BTreeMap::<Option<u32>, FocusTotals>::new();
        for session in &self.saved.sessions {
            let entry = days.entry(session.date).or_insert(Day {
                date: session.date,
                count: 0,
                minutes: 0,
            });
            entry.count += 1;
            entry.minutes += u64::from(session.minutes);
            let totals = focus_by_todo.entry(session.todo_id).or_default();
            totals.count += 1;
            totals.minutes += u64::from(session.minutes);
        }
        let todos = self
            .saved
            .todos
            .iter()
            .map(|todo| {
                let totals = focus_by_todo
                    .get(&Some(todo.id))
                    .copied()
                    .unwrap_or_default();
                TodoSummary {
                    todo: todo.clone(),
                    count: totals.count,
                    minutes: totals.minutes,
                }
            })
            .collect();
        Summary {
            settings: self.saved.settings,
            today: days[&today].clone(),
            days: days.into_values().rev().collect(),
            goal: self.goal_progress(),
            todos,
            unassigned: focus_by_todo.remove(&None).unwrap_or_default(),
        }
    }

    fn start_focus(
        &mut self,
        minutes: u16,
        rest_minutes: u16,
        todo_id: Option<u32>,
        now_ms: u64,
    ) -> Result<(), &'static str> {
        if !matches!(self.saved.stage, Stage::Idle) {
            return Err("计时状态已更新，请再试一次。");
        }
        if !(1..=180).contains(&minutes) || !(1..=60).contains(&rest_minutes) {
            return Err("专注可设为1分至3小时，休息可设为1分至1小时。");
        }
        let timer = Countdown::new(minutes);
        self.saved.settings.focus_minutes = minutes;
        self.saved.settings.rest_minutes = rest_minutes;
        self.saved.stage = Stage::Focus { timer, todo_id };
        self.deadline_ms = Some(now_ms.saturating_add(timer.remaining_ms));
        Ok(())
    }

    fn todo_mut(&mut self, id: u32) -> Result<&mut Todo, &'static str> {
        self.saved
            .todos
            .iter_mut()
            .find(|todo| todo.id == id)
            .ok_or("找不到这项待办。")
    }

    fn save_todo(
        &mut self,
        id: Option<u32>,
        name: String,
        due_at: Option<NaiveDateTime>,
    ) -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err("请输入 1–80 个字符的待办名称。");
        }
        if let Some(id) = id {
            let todo = self.todo_mut(id)?;
            todo.name = name.to_owned();
            todo.due_at = due_at;
        } else {
            let id = self
                .saved
                .todos
                .last()
                .map_or(0, |todo| todo.id)
                .checked_add(1)
                .ok_or("待办数量已达上限。")?;
            self.saved.todos.push(Todo {
                id,
                name: name.to_owned(),
                due_at,
                completed: false,
            });
        }
        Ok(())
    }

    fn set_goal(&mut self, name: String, minutes: u32) -> Result<(), &'static str> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err("请输入 1–60 个字符的奖励名称。");
        }
        if !(1..=60_000).contains(&minutes) {
            return Err("目标时长可设为1分至41天16小时。");
        }
        let first_session = self
            .saved
            .goal
            .as_ref()
            .filter(|goal| goal.claimed_minutes.is_none())
            .map_or(self.saved.sessions.len(), |goal| goal.first_session);
        self.saved.goal = Some(Goal {
            name: name.to_owned(),
            minutes,
            first_session,
            claimed_minutes: None,
        });
        Ok(())
    }

    fn goal_progress(&self) -> Option<GoalProgress> {
        let goal = self.saved.goal.as_ref()?;
        let earned = goal.claimed_minutes.unwrap_or_else(|| {
            self.saved
                .sessions
                .iter()
                .skip(goal.first_session)
                .map(|session| u64::from(session.minutes))
                .sum()
        });
        Some(GoalProgress {
            name: goal.name.clone(),
            target_minutes: goal.minutes,
            earned_minutes: earned,
            reached: earned >= u64::from(goal.minutes),
            claimed: goal.claimed_minutes.is_some(),
        })
    }
}
