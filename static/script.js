const form = document.getElementById("pdf-form");
const input = document.getElementById("card-ids-input");
const result = document.getElementById("result");
const resultPanel = document.getElementById("result-panel");
const cardCount = document.getElementById("card-count");
const generateButton = document.getElementById("generate-button");
const clearButton = document.getElementById("clear-button");

if (
  form &&
  input &&
  result &&
  resultPanel &&
  cardCount &&
  generateButton &&
  clearButton
) {
  form.addEventListener("submit", generatePdf);
  input.addEventListener("input", syncInputMeta);
  clearButton.addEventListener("click", clearForm);
  syncInputMeta();
}

async function generatePdf(event) {
  event.preventDefault();

  const cardIds = collectCardIds(input.value);

  if (cardIds.length === 0) {
    setResultState("error", "Введите хотя бы один ID карты.");
    return;
  }

  setLoadingState(true);
  setResultState("loading", `Генерация PDF для ${cardIds.length} ID...`);

  try {
    const path = await requestPdf(cardIds);
    showDownloadLink(path, cardIds.length);
  } catch (error) {
    console.error(error);
    setResultState("error", error.message || "Не удалось выполнить запрос.");
  } finally {
    setLoadingState(false);
  }
}

async function requestPdf(cardIds) {
  const res = await fetch("/pdf", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ card_ids: cardIds }),
  });

  if (!res.ok) {
    const message = (await res.text()).trim();
    throw new Error(message || "Не удалось выполнить запрос");
  }

  const data = await res.json();
  return data.path;
}

function collectCardIds(text) {
  return text
    .split(/[\s,;]+/)
    .map((part) => part.trim())
    .filter(Boolean);
}

function showDownloadLink(path, count) {
  clearResultState();
  resultPanel.classList.add("result-success");

  const summary = document.createElement("p");
  summary.textContent = `Готово. Сформирован PDF для ${count} ID.`;

  const link = document.createElement("a");
  link.className = "result-link";
  link.href = path;
  link.textContent = "Открыть PDF в новой вкладке";
  link.target = "_blank";
  link.rel = "noopener noreferrer";

  result.replaceChildren(summary, link);
}

function clearForm() {
  input.value = "";
  syncInputMeta();
  input.focus();
  setResultState("idle", "Введите ID карт и нажмите «Сформировать PDF».");
}

function syncInputMeta() {
  const cardIds = collectCardIds(input.value);
  cardCount.textContent = `${cardIds.length} ID`;
}

function setLoadingState(isLoading) {
  generateButton.disabled = isLoading;
  clearButton.disabled = isLoading;
  generateButton.textContent = isLoading ? "Формируем..." : "Сформировать PDF";
}

function setResultState(state, message) {
  clearResultState();
  if (state === "loading") {
    resultPanel.classList.add("result-loading");
  }
  if (state === "error") {
    resultPanel.classList.add("result-error");
  }
  if (state === "idle") {
    resultPanel.classList.add("result-idle");
  }
  result.textContent = message;
}

function clearResultState() {
  resultPanel.classList.remove("result-idle", "result-loading", "result-success", "result-error");
}

if (resultPanel && result && input) {
  setResultState("idle", "Введите ID карт и нажмите «Сформировать PDF».");
}

if (input) {
  input.placeholder = "1\n12\n203\n311";
}
