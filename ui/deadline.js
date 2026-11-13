'use strict';

function createDeadlinePicker() {
  const element = (id) => document.getElementById(id);
  const dialog = element('deadline-dialog');
  const trigger = element('todo-due-button');
  const days = element('deadline-days');
  const dateInput = element('deadline-date');
  const timeInput = element('deadline-time');
  const previous = element('deadline-previous');
  const next = element('deadline-next');
  const error = element('deadline-error');
  const clear = element('deadline-clear');
  const dateText = element('deadline-value');
  const timeText = element('deadline-clock');
  const dateLabel = new Intl.DateTimeFormat('zh-CN', {
    timeZone: 'UTC', year: 'numeric', month: 'long', day: 'numeric', weekday: 'short'
  });
  let value = null;
  let today;
  let month;

  // Use UTC for calendar arithmetic so a local clock change cannot skip a date.
  function civilDate(year, monthIndex, day) {
    const date = new Date(0);
    date.setUTCFullYear(year, monthIndex, day);
    return date;
  }

  function parseDate(text) {
    const parts = /^(\d{4})[-/](\d{1,2})[-/](\d{1,2})$/.exec(text.trim());
    if (!parts) return null;
    const [year, monthNumber, day] = parts.slice(1).map(Number);
    const date = civilDate(year, monthNumber - 1, day);
    return year > 0 && date.getUTCFullYear() === year &&
      date.getUTCMonth() === monthNumber - 1 && date.getUTCDate() === day ? date : null;
  }

  function parseTime(text) {
    const parts = /^(\d{1,2}):(\d{1,2})$/.exec(text.trim());
    if (!parts || Number(parts[1]) > 23 || Number(parts[2]) > 59) return null;
    return parts.slice(1).map((part) => part.padStart(2, '0')).join(':');
  }

  function key(date) {
    return date.toISOString().slice(0, 10);
  }

  function viewMonth(date) {
    month = civilDate(date.getUTCFullYear(), date.getUTCMonth(), 1);
  }

  function renderCalendar(focusDate = null) {
    const year = month.getUTCFullYear();
    const monthIndex = month.getUTCMonth();
    const monthKey = key(month).slice(0, 7);
    const todayKey = key(today);
    const selected = parseDate(dateInput.value);
    const selectedKey = selected ? key(selected) : '';
    const focusKey = focusDate ? key(focusDate) : [selectedKey, todayKey, key(month)].find((date) => date.startsWith(monthKey));
    element('deadline-month').textContent = year + '年' + (monthIndex + 1) + '月';
    previous.disabled = year === 1 && monthIndex === 0;
    next.disabled = year === 9999 && monthIndex === 11;

    const cells = document.createDocumentFragment();
    const offset = (month.getUTCDay() + 6) % 7;
    for (let i = 0; i < offset; i++) {
      const empty = document.createElement('span');
      empty.setAttribute('aria-hidden', 'true');
      cells.append(empty);
    }
    const last = civilDate(year, monthIndex + 1, 0).getUTCDate();
    for (let day = 1; day <= last; day++) {
      const date = civilDate(year, monthIndex, day);
      const dateKey = key(date);
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'calendar-day';
      button.textContent = String(day);
      button.dataset.date = dateKey;
      button.tabIndex = dateKey === focusKey ? 0 : -1;
      button.setAttribute('aria-label', dateLabel.format(date));
      button.setAttribute('aria-pressed', String(dateKey === selectedKey));
      if (dateKey === todayKey) button.setAttribute('aria-current', 'date');
      cells.append(button);
    }
    days.replaceChildren(cells);
    if (focusDate) days.querySelector('[tabindex="0"]').focus();
  }

  function clearError() {
    error.hidden = true;
    error.textContent = '';
    dateInput.removeAttribute('aria-invalid');
    timeInput.removeAttribute('aria-invalid');
  }

  function invalid(input, message) {
    error.textContent = message;
    error.hidden = false;
    input.setAttribute('aria-invalid', 'true');
    input.focus();
  }

  function select(date) {
    dateInput.value = key(date);
    clearError();
    viewMonth(date);
    renderCalendar(date);
  }

  function setValue(nextValue) {
    value = nextValue;
    const date = value ? parseDate(value.slice(0, 10)) : null;
    dateText.textContent = date ? dateLabel.format(date) : '选择日期和时间';
    timeText.textContent = date ? value.slice(11, 16) : '';
    timeText.hidden = !date;
    trigger.classList.toggle('has-deadline', Boolean(date));
  }

  function close() {
    if (dialog.open) dialog.close();
  }

  trigger.addEventListener('click', () => {
    const now = new Date();
    today = civilDate(now.getFullYear(), now.getMonth(), now.getDate());
    const date = (value && parseDate(value.slice(0, 10))) || today;
    dateInput.value = key(date);
    timeInput.value = value ? value.slice(11, 16) : '23:59';
    clear.hidden = !value;
    clearError();
    viewMonth(date);
    renderCalendar();
    dialog.showModal();
    trigger.setAttribute('aria-expanded', 'true');
    days.querySelector('[tabindex="0"]').focus();
  });
  dialog.addEventListener('close', () => {
    if (!dialog.open) trigger.setAttribute('aria-expanded', 'false');
  });
  dialog.addEventListener('click', (event) => {
    if (event.target !== dialog) return;
    const rect = dialog.getBoundingClientRect();
    if (event.clientX < rect.left || event.clientX > rect.right ||
        event.clientY < rect.top || event.clientY > rect.bottom) close();
  });

  previous.addEventListener('click', () => {
    month = civilDate(month.getUTCFullYear(), month.getUTCMonth() - 1, 1);
    renderCalendar();
  });
  next.addEventListener('click', () => {
    month = civilDate(month.getUTCFullYear(), month.getUTCMonth() + 1, 1);
    renderCalendar();
  });
  element('deadline-today').addEventListener('click', () => select(today));
  days.addEventListener('click', (event) => {
    if (event.target.dataset.date) select(parseDate(event.target.dataset.date));
  });
  days.addEventListener('keydown', (event) => {
    const offset = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[event.key];
    if (offset === undefined || !event.target.dataset.date) return;
    event.preventDefault();
    const date = parseDate(event.target.dataset.date);
    date.setUTCDate(date.getUTCDate() + offset);
    if (date.getUTCFullYear() < 1 || date.getUTCFullYear() > 9999) return;
    viewMonth(date);
    renderCalendar(date);
  });
  dateInput.addEventListener('input', () => {
    clearError();
    const date = parseDate(dateInput.value);
    if (date) {
      viewMonth(date);
      renderCalendar();
    }
  });
  timeInput.addEventListener('input', clearError);
  element('deadline-cancel').addEventListener('click', close);
  clear.addEventListener('click', () => {
    setValue(null);
    close();
  });
  element('deadline-form').addEventListener('submit', (event) => {
    event.preventDefault();
    clearError();
    const date = parseDate(dateInput.value);
    if (!date) return invalid(dateInput, '输入有效日期，例如 2026-11-13。');
    const time = parseTime(timeInput.value);
    if (!time) return invalid(timeInput, '使用 24 小时制，例如 17:30。');
    setValue(key(date) + 'T' + time + ':00');
    close();
  });

  return {
    get value() { return value; },
    set value(nextValue) { setValue(nextValue); },
    close
  };
}
