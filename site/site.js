"use strict";

const field = document.getElementById("key");
const action = document.getElementById("key-action");
const status = document.getElementById("key-status");
const formatDate = new Intl.DateTimeFormat("ru-RU", {
  timeZone: "Europe/Moscow", day: "numeric", month: "long", hour: "2-digit", minute: "2-digit"
});
let active = null;
let deadline = null;

for (const link of document.querySelectorAll(".release-link")) link.href = window.twiReleaseUrl ?? (window.twiRepo + "/releases/latest");
for (const link of document.querySelectorAll(".repo-link")) link.href = window.twiRepo;

async function reveal() {
  action.disabled = true;
  status.textContent = "Загружаем актуальный ключ…";

  try {
    const response = await fetch(new URL("keys.json", document.baseURI), { cache: "no-store" });
    if (!response.ok) throw new Error("Key feed unavailable");
    const { keys } = await response.json();
    const now = Math.floor(Date.now() / 1000);
    const selected = Array.isArray(keys) && keys.find(({ value, starts, ends }) =>
      starts <= now && now < ends && ends - starts === 86400 &&
      typeof value === "string" && /^TWI1\.\d+\.\d+\.[\w-]{342}$/.test(value)
    );

    if (!selected) throw new Error("No valid key");

    active = selected;
    field.value = active.value;
    action.textContent = "Скопировать";
    status.textContent = "Действует до " + formatDate.format(new Date(active.ends * 1000)) + " мск.";
    clearTimeout(deadline);
    deadline = setTimeout(resetKey, Math.max(0, active.ends * 1000 - Date.now()));
  } catch {
    resetKey();
    status.textContent = "Ключ пока недоступен. Проверьте подключение и попробуйте ещё раз.";
  } finally {
    action.disabled = false;
  }
}

function resetKey() {
  active = null;
  field.value = "";
  action.textContent = "Показать ключ";
  status.textContent = "Один ключ действует до ближайшей полуночи по Москве.";
}

action.addEventListener("click", async () => {
  if (!active || Date.now() >= active.ends * 1000) {
    await reveal();
    return;
  }

  try {
    await navigator.clipboard.writeText(active.value);
    status.textContent = "Ключ скопирован. Вставьте его в приложение.";
  } catch {
    field.focus();
    field.select();
    status.textContent = "Ключ выделен. Нажмите Ctrl+C, чтобы скопировать.";
  }
});
