"use strict";

const openButton = document.getElementById("open-captcha");
const form = document.getElementById("captcha");
const image = document.getElementById("captcha-image");
const answer = document.getElementById("answer");
const refreshButton = document.getElementById("refresh-captcha");
const submitButton = document.getElementById("submit-answer");
const result = document.getElementById("key-result");
const field = document.getElementById("key");
const copyButton = document.getElementById("copy-key");
const status = document.getElementById("key-status");
const expiryLabel = "Ключ действует до ближайшей полуночи по Москве.";
const formatDate = new Intl.DateTimeFormat("ru-RU", {
  timeZone: "Europe/Moscow", day: "numeric", month: "long", hour: "2-digit", minute: "2-digit"
});
let ticket = null;
let expires = null;

for (const link of document.querySelectorAll(".release-link")) link.href = window.twiRepo + "/releases/latest";
for (const link of document.querySelectorAll(".repo-link")) link.href = window.twiRepo;

async function post(path, data) {
  return fetch(new URL(path, window.twiKeyApi), {
    method: "POST",
    cache: "no-store",
    headers: data ? { "Content-Type": "application/json" } : undefined,
    body: data ? JSON.stringify(data) : undefined
  });
}

function busy(active) {
  openButton.disabled = refreshButton.disabled = answer.disabled = active;
  submitButton.disabled = active || !ticket;
}

function hideResult() {
  expires = null;
  field.value = "";
  result.hidden = true;
  openButton.hidden = false;
  status.textContent = expiryLabel;
}

async function loadCaptcha(message = "Введите символы с картинки.") {
  busy(true);
  status.textContent = "Загружаем проверку…";
  ticket = null;
  answer.value = "";

  try {
    const response = await post("/api/challenge");
    if (!response.ok) throw new Error("Challenge unavailable");
    const data = await response.json();
    if (typeof data.ticket !== "string" || !data.ticket ||
        typeof data.image !== "string" || !data.image.startsWith("data:image/png;base64,")) {
      throw new Error("Invalid challenge");
    }

    ticket = data.ticket;
    image.src = data.image;
    openButton.hidden = true;
    form.hidden = false;
    status.textContent = message;
  } catch {
    form.hidden = true;
    openButton.hidden = false;
    status.textContent = "CAPTCHA сейчас недоступна. Попробуйте ещё раз.";
  } finally {
    busy(false);
    if (ticket) answer.focus();
  }
}

openButton.addEventListener("click", () => loadCaptcha());
refreshButton.addEventListener("click", () => loadCaptcha());
form.addEventListener("submit", async event => {
  event.preventDefault();
  if (!ticket || submitButton.disabled) return;
  const code = answer.value.trim().toUpperCase();
  if (!/^[2-9A-HJ-NP-Z]{6}$/.test(code)) {
    status.textContent = "Введите шесть латинских букв или цифр с картинки.";
    answer.focus();
    return;
  }

  busy(true);
  status.textContent = "Проверяем код…";
  try {
    const response = await post("/api/key", { ticket, answer: code });
    if (response.status === 422) {
      await loadCaptcha("Код неверный или время вышло. Введите код с новой картинки.");
      return;
    }

    if (!response.ok) throw new Error("Key unavailable");
    const { value, ends } = await response.json();
    if (!/^TWI1\.\d+\.\d+\.[\w-]{342}$/.test(value) || !Number.isSafeInteger(ends) || ends * 1000 <= Date.now()) {
      throw new Error("Invalid key");
    }

    ticket = null;
    expires = ends * 1000;
    field.value = value;
    form.hidden = true;
    result.hidden = false;
    status.textContent = "Действует до " + formatDate.format(expires) + " мск.";
    setTimeout(hideResult, expires - Date.now());
    copyButton.focus();
  } catch {
    status.textContent = "Сервис не выдал ключ. Попробуйте ещё раз.";
  } finally {
    busy(false);
  }
});

copyButton.addEventListener("click", async () => {
  if (!expires || Date.now() >= expires) {
    hideResult();
    return;
  }

  try {
    await navigator.clipboard.writeText(field.value);
    status.textContent = "Ключ скопирован. Вставьте его в приложение.";
  } catch {
    field.focus();
    field.select();
    status.textContent = "Ключ выделен. Нажмите Ctrl+C, чтобы скопировать.";
  }
});
