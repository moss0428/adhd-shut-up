'use strict';

const byId = (id) => document.getElementById(id);
const durationButtons = [...document.querySelectorAll('[data-duration]')];
const historyButtons = [...document.querySelectorAll('[data-history-mode]')];
const pages = [...document.querySelectorAll('[data-view]')];
const focusInput = byId('focus-minutes');
const restInput = byId('rest-minutes');
const durationForm = byId('duration-form');
const focusTodo = byId('focus-todo');
const focusAudioEnabled = byId('focus-audio-enabled');
const focusAudioTrack = byId('focus-audio-track');
const primary = byId('primary-action');
const secondary = byId('secondary-action');
const todoForm = byId('todo-form');
const todoName = byId('todo-name');
const deadlinePicker = createDeadlinePicker();
const newTodo = byId('new-todo');
const saveTodo = byId('save-todo');
const goalForm = byId('goal-form');
const goalItem = byId('goal-item');
const goalDuration = byId('goal-duration');
const goalUnit = byId('goal-unit');
let clock;
let state;
let customDuration = false;
let chosenMinutes = 15;
let historyMode = 'day';
let editingTodo = null;
let savingTodo = false;
let editingGoal = false;
let savingGoal = false;

function send(action, fields = {}) {
  window.webkit.messageHandlers.shuidi.postMessage(JSON.stringify({ action, ...fields }));
}

window.shuidiShowPage = (page) => {
  if (page !== 'todos') deadlinePicker.close();
  for (const section of pages) section.hidden = section.dataset.view !== page;
  window.scrollTo(0, 0);
};

function durationParts(totalMinutes, countdown = false) {
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor(totalMinutes / 60) % 24;
  const minutes = totalMinutes % 60;
  const parts = [];
  if (days) parts.push([String(days), '天']);
  if (hours || (countdown && days)) parts.push([String(hours), '小时']);
  if (countdown) parts.push([String(minutes).padStart(2, '0'), '分']);
  else if (minutes || !parts.length) parts.push([String(minutes), '分']);
  return parts;
}

function formatMinutes(minutes) {
  return durationParts(minutes).map(([number, unit]) => number + unit).join('');
}

function renderCountdown(seconds, live) {
  const countdown = byId('countdown');
  const parts = durationParts(Math.floor(seconds / 60), live);
  if (live) parts.push([String(seconds % 60).padStart(2, '0'), '秒']);
  const label = parts.map(([number, unit]) => number + unit).join('');
  if (countdown.getAttribute('aria-label') === label) return;
  countdown.setAttribute('aria-label', label);
  countdown.replaceChildren(...parts.map(([number, unit]) => {
    const segment = document.createElement('span');
    const suffix = document.createElement('span');
    suffix.className = 'time-unit';
    suffix.textContent = unit;
    segment.append(number, suffix);
    return segment;
  }));
}

function showNotice(text) {
  const notice = byId('notice');
  notice.textContent = text || '';
  notice.hidden = !text;
}

function selectDuration(value) {
  customDuration = value === 'custom';
  if (!customDuration) {
    chosenMinutes = Number(value);
    focusInput.value = chosenMinutes;
  }
  byId('custom-duration').hidden = !customDuration;
  focusInput.disabled = !customDuration;
  for (const button of durationButtons) {
    const selected = customDuration ? button.dataset.duration === 'custom' : Number(button.dataset.duration) === chosenMinutes;
    button.setAttribute('aria-pressed', String(selected));
  }
  if (clock?.phase === 'idle') updateClock(clock);
}

const modeTitle = {
  idle: '专注计时',
  focus: '专注中',
  focus_paused: '专注已暂停',
  completed: '🎉 专注完成',
  rest: '休息中',
  rest_paused: '休息已暂停'
};

function updateClock(nextClock) {
  clock = nextClock;
  const phase = clock.phase;
  const idle = phase === 'idle';
  const complete = phase === 'completed';
  const paused = phase.endsWith('_paused');
  const rest = phase.startsWith('rest');
  const minutes = customDuration ? focusInput.valueAsNumber : chosenMinutes;
  let seconds = clock.seconds;
  if (idle && Number.isInteger(minutes) && minutes > 0 && minutes <= 180) seconds = minutes * 60;
  if (complete) seconds = clock.total_seconds;
  renderCountdown(seconds, !idle && !complete);
  const title = byId('phase-title');
  if (title.textContent !== modeTitle[phase]) title.textContent = modeTitle[phase];
  durationForm.hidden = !idle;
  const activeTodo = state?.todos.find((todo) => todo.id === clock.todo_id);
  const taskName = byId('focus-todo-name');
  taskName.hidden = !activeTodo;
  if (taskName.textContent !== (activeTodo?.name ?? '')) taskName.textContent = activeTodo?.name ?? '';
  const track = byId('timer-progress');
  track.hidden = idle || complete;
  const elapsed = clock.total_seconds ? 100 * (1 - clock.seconds / clock.total_seconds) : 0;
  const percent = Math.max(0, Math.min(100, elapsed));
  track.firstElementChild.style.width = percent + '%';
  track.setAttribute('aria-valuenow', String(Math.round(percent)));
  primary.textContent = idle ? '开始专注' : complete ? '休息 ' + formatMinutes(state.settings.rest_minutes) : paused ? '继续计时' : '暂停';
  primary.disabled = false;
  secondary.hidden = idle;
  secondary.textContent = complete ? '先不休息' : rest ? '结束休息' : '结束本次';
}

const dateFormatters = {
  year: new Intl.DateTimeFormat('zh-CN', { timeZone: 'UTC', year: 'numeric', month: 'long', day: 'numeric' }),
  day: new Intl.DateTimeFormat('zh-CN', { timeZone: 'UTC', month: 'long', day: 'numeric', weekday: 'short' })
};

function dateLabel(date, today) {
  if (date === today) return '今天';
  const formatter = date.slice(0, 4) === today.slice(0, 4) ? dateFormatters.day : dateFormatters.year;
  return formatter.format(new Date(date + 'T00:00:00Z'));
}

function orderedTodos() {
  return [...state.todos].sort((a, b) => {
    if (a.completed !== b.completed) return a.completed ? 1 : -1;
    if (a.due_at && b.due_at) return a.due_at.localeCompare(b.due_at) || a.id - b.id;
    if (a.due_at !== b.due_at) return a.due_at ? -1 : 1;
    return a.id - b.id;
  });
}

function renderHistory() {
  if (!state) return;
  const { today, days } = state;
  const byTodo = historyMode === 'todo';
  const items = byTodo ? orderedTodos() : days;
  if (byTodo && state.unassigned.count) items.push({ ...state.unassigned, name: '未关联待办' });
  byId('history-label').textContent = byTodo ? '待办' : '日期';
  byId('history-table').hidden = !items.length;
  byId('history-empty').hidden = Boolean(items.length);
  const rows = document.createDocumentFragment();
  for (const item of items) {
    const row = document.createElement('tr');
    if (!item.count) row.className = 'zero';
    const name = byTodo ? item.name : dateLabel(item.date, today.date);
    for (const text of [name, item.count + ' 次', formatMinutes(item.minutes)]) {
      row.insertCell().textContent = text;
    }
    rows.append(row);
  }
  byId('history-rows').replaceChildren(rows);
}

function openTodoForm(todo) {
  if (savingTodo) return;
  editingTodo = todo?.id ?? null;
  todoName.value = todo?.name ?? '';
  deadlinePicker.value = todo?.due_at ?? null;
  todoName.setCustomValidity('');
  todoForm.hidden = false;
  newTodo.setAttribute('aria-expanded', 'true');
  todoName.focus();
}

function closeTodoForm() {
  deadlinePicker.close();
  todoForm.hidden = true;
  editingTodo = null;
  newTodo.setAttribute('aria-expanded', 'false');
  newTodo.focus({ preventScroll: true });
}

function todoButton(text, todo, onClick) {
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'quiet';
  button.textContent = text;
  button.setAttribute('aria-label', text + ' ' + todo.name);
  button.dataset.todoControl = text + ':' + todo.id;
  button.addEventListener('click', onClick);
  return button;
}

function todoRow(todo) {
  const row = document.createElement('li');
  row.className = todo.completed ? 'todo-row is-complete' : 'todo-row';
  const check = document.createElement('input');
  check.type = 'checkbox';
  check.checked = todo.completed;
  check.setAttribute('aria-label', '完成 ' + todo.name);
  check.dataset.todoControl = '完成:' + todo.id;
  check.addEventListener('change', () => send('set_todo_completed', { id: todo.id, completed: check.checked }));

  const details = document.createElement('div');
  const name = document.createElement('div');
  name.className = 'todo-name';
  name.textContent = todo.name;
  details.append(name);
  if (todo.due_at) {
    const due = document.createElement('time');
    due.dateTime = todo.due_at;
    due.textContent = '截止 ' + todo.due_at.slice(0, 10).replaceAll('-', '/') + ' ' + todo.due_at.slice(11, 16);
    details.append(due);
  }

  const actions = document.createElement('div');
  actions.className = 'todo-row-actions';
  if (!todo.completed) {
    const focus = todoButton('去专注', todo, () => {
      focusTodo.value = String(todo.id);
      window.webkit.messageHandlers.shuidi.postMessage('show_focus');
    });
    focus.disabled = state.clock.phase !== 'idle';
    actions.append(focus);
  }
  actions.append(todoButton('编辑', todo, () => openTodoForm(todo)));
  row.append(check, details, actions);
  return row;
}

function renderTodos() {
  const list = byId('todo-list');
  const focused = document.activeElement?.dataset.todoControl;
  const selected = focusTodo.value;
  const options = document.createDocumentFragment();
  const rows = document.createDocumentFragment();
  options.append(new Option('不选待办', ''));
  for (const todo of orderedTodos()) {
    rows.append(todoRow(todo));
    if (!todo.completed) options.append(new Option(todo.name, String(todo.id)));
  }
  focusTodo.replaceChildren(options);
  focusTodo.value = selected;
  if (focusTodo.selectedIndex < 0) focusTodo.value = '';
  list.replaceChildren(rows);
  if (focused) {
    const control = [...list.querySelectorAll('[data-todo-control]')].find((element) => element.dataset.todoControl === focused);
    control?.focus({ preventScroll: true });
  }
  byId('todos-empty').hidden = Boolean(state.todos.length);
}

function renderGoal() {
  const goal = state.goal;
  const showForm = editingGoal || !goal;
  byId('goal-summary').hidden = showForm;
  goalForm.hidden = !showForm;
  byId('cancel-goal').hidden = !goal;
  byId('goal-note').textContent = editingGoal && goal && !goal.claimed
    ? '修改目标后，已累计的专注时长继续计入。'
    : '从设定目标起，累计完成的专注时长。';
  byId('reward-preview-title').textContent = goal?.name || '设定奖励';
  const preview = byId('reward-preview-progress');
  preview.hidden = !goal;
  if (!goal) return;

  const goalState = goal.claimed ? '已领取' : goal.reached ? '已达成，可以买啦' : '累计专注';
  const progress = formatMinutes(goal.earned_minutes) + ' / ' + formatMinutes(goal.target_minutes);
  preview.textContent = goal.claimed || goal.reached ? goalState : progress;
  byId('goal-state').textContent = goalState;
  byId('goal-name').textContent = goal.name;
  const meter = byId('goal-meter');
  meter.max = goal.target_minutes;
  meter.value = Math.min(goal.earned_minutes, goal.target_minutes);
  byId('goal-progress-text').textContent = progress;
  byId('claim-goal').hidden = !goal.reached || goal.claimed;
  byId('edit-goal').textContent = goal.claimed ? '设置新目标' : '编辑目标';
}

window.shuidiSetState = (nextState) => {
  const previous = state?.settings;
  state = nextState;
  if (!previous || previous.focus_minutes !== state.settings.focus_minutes || previous.rest_minutes !== state.settings.rest_minutes) {
    chosenMinutes = state.settings.focus_minutes;
    focusInput.value = chosenMinutes;
    restInput.value = state.settings.rest_minutes;
    selectDuration([15, 30].includes(chosenMinutes) ? String(chosenMinutes) : 'custom');
  }
  const audio = state.settings.focus_audio;
  focusAudioEnabled.checked = audio.enabled;
  focusAudioTrack.value = audio.track;
  focusAudioEnabled.disabled = false;
  focusAudioTrack.disabled = false;
  if (savingGoal) {
    if (!state.notice) editingGoal = false;
    savingGoal = false;
  }
  if (savingTodo) {
    savingTodo = false;
    saveTodo.disabled = false;
    if (!state.notice) closeTodoForm();
  }
  showNotice(state.notice);
  updateClock(state.clock);
  renderTodos();
  renderHistory();
  renderGoal();
};

window.shuidiSetClock = updateClock;

function saveFocusAudio() {
  send('set_focus_audio', { enabled: focusAudioEnabled.checked, track: focusAudioTrack.value });
}

function startFocus() {
  if (!durationForm.reportValidity()) return;
  const fields = {
    minutes: customDuration ? focusInput.valueAsNumber : chosenMinutes,
    rest_minutes: restInput.valueAsNumber
  };
  if (focusTodo.value) send('start_todo_focus', { ...fields, todo_id: Number(focusTodo.value) });
  else send('start_focus', fields);
}

byId('open-reward').addEventListener('click', () => window.webkit.messageHandlers.shuidi.postMessage('show_reward'));
focusAudioEnabled.addEventListener('change', saveFocusAudio);
focusAudioTrack.addEventListener('change', saveFocusAudio);
for (const button of historyButtons) {
  button.addEventListener('click', () => {
    historyMode = button.dataset.historyMode;
    for (const option of historyButtons) option.setAttribute('aria-pressed', String(option === button));
    renderHistory();
  });
}
newTodo.addEventListener('click', () => openTodoForm());
byId('cancel-todo').addEventListener('click', () => { if (!savingTodo) closeTodoForm(); });
todoName.addEventListener('input', () => todoName.setCustomValidity(''));
todoForm.addEventListener('submit', (event) => {
  event.preventDefault();
  if (savingTodo) return;
  const name = todoName.value.trim();
  if (!name || [...name].length > 80) {
    todoName.setCustomValidity('请输入 1–80 个字符的待办名称。');
    todoName.reportValidity();
    return;
  }
  savingTodo = true;
  saveTodo.disabled = true;
  send('save_todo', { id: editingTodo, name, due_at: deadlinePicker.value });
});
for (const button of durationButtons) {
  button.addEventListener('click', () => {
    selectDuration(button.dataset.duration);
    if (customDuration) {
      focusInput.focus();
      focusInput.select();
    }
  });
}
focusInput.addEventListener('input', () => { if (clock) updateClock(clock); });
durationForm.addEventListener('submit', (event) => {
  event.preventDefault();
  if (clock?.phase === 'idle') startFocus();
});
primary.addEventListener('click', () => {
  if (!clock) return;
  if (clock.phase === 'idle') startFocus();
  else if (clock.phase === 'completed') send('start_rest');
  else if (clock.phase.endsWith('_paused')) send('resume');
  else send('pause');
});
secondary.addEventListener('click', () => send(clock.phase === 'completed' ? 'skip_rest' : 'stop'));
byId('claim-goal').addEventListener('click', () => send('claim_goal'));
byId('edit-goal').addEventListener('click', () => {
  editingGoal = true;
  const goal = state.goal;
  const value = goal && !goal.claimed ? goal.target_minutes : 0;
  goalItem.value = value ? goal.name : '';
  goalItem.setCustomValidity('');
  goalUnit.value = value % 60 ? '1' : '60';
  goalDuration.max = 60000 / Number(goalUnit.value);
  goalDuration.value = value ? value / Number(goalUnit.value) : '';
  renderGoal();
  goalItem.focus();
});
byId('cancel-goal').addEventListener('click', () => {
  editingGoal = false;
  renderGoal();
});
goalUnit.addEventListener('change', () => { goalDuration.max = 60000 / Number(goalUnit.value); });
goalItem.addEventListener('input', () => goalItem.setCustomValidity(''));
goalForm.addEventListener('submit', (event) => {
  event.preventDefault();
  const name = goalItem.value.trim();
  if (!name) {
    goalItem.setCustomValidity('请输入奖励名称。');
    goalItem.reportValidity();
    return;
  }
  savingGoal = true;
  send('set_goal', { name, minutes: goalDuration.valueAsNumber * Number(goalUnit.value) });
});

window.webkit.messageHandlers.shuidi.postMessage('ready');
