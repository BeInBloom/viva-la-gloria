const PAGE_SIZE = 24;
const DEFAULT_RESULT_MESSAGE = "Выберите карты и нажмите «Сформировать PDF».";
const CLEARED_SELECTION_MESSAGE = "Выбор очищен. Добавьте карты и сформируйте новый PDF.";

const catalogGrid = document.getElementById("catalog-grid");
const catalogStatus = document.getElementById("catalog-status");
const catalogCount = document.getElementById("catalog-count");
const previousPageButton = document.getElementById("previous-page-button");
const nextPageButton = document.getElementById("next-page-button");
const pageIndicator = document.getElementById("page-indicator");
const selectionPanel = document.getElementById("selection-panel");
const selectionToggleButton = document.getElementById("selection-toggle-button");
const selectionList = document.getElementById("selection-list");
const selectionEmpty = document.getElementById("selection-empty");
const selectionBadge = document.getElementById("selection-badge");
const selectionUniqueCount = document.getElementById("selection-unique-count");
const selectionTotalCount = document.getElementById("selection-total-count");
const generateButton = document.getElementById("generate-button");
const clearButton = document.getElementById("clear-button");
const manualPanel = document.getElementById("manual-panel");
const manualToggleButton = document.getElementById("manual-toggle-button");
const manualInput = document.getElementById("card-ids-input");
const manualCount = document.getElementById("manual-count");
const manualApplyButton = document.getElementById("manual-apply-button");
const result = document.getElementById("result");
const resultPanel = document.getElementById("result-panel");

const state = {
  catalogItems: [],
  catalogIndex: new Map(),
  catalogPages: [],
  currentCatalogPage: 0,
  hasLoadedCatalog: false,
  isCatalogLoading: false,
  catalogError: "",
  selection: new Map(),
  isGenerating: false,
  activeSidebarPanel: "selection",
};

ensureRequiredElements();
bindEvents();

manualInput.placeholder = "1\n12\n203\n311";
renderApp();
setResultState("idle", DEFAULT_RESULT_MESSAGE);
loadInitialCards();

function ensureRequiredElements() {
  const missingElements = [
    ["catalogGrid", catalogGrid],
    ["catalogStatus", catalogStatus],
    ["catalogCount", catalogCount],
    ["previousPageButton", previousPageButton],
    ["nextPageButton", nextPageButton],
    ["pageIndicator", pageIndicator],
    ["selectionPanel", selectionPanel],
    ["selectionToggleButton", selectionToggleButton],
    ["selectionList", selectionList],
    ["selectionEmpty", selectionEmpty],
    ["selectionBadge", selectionBadge],
    ["selectionUniqueCount", selectionUniqueCount],
    ["selectionTotalCount", selectionTotalCount],
    ["generateButton", generateButton],
    ["clearButton", clearButton],
    ["manualPanel", manualPanel],
    ["manualToggleButton", manualToggleButton],
    ["manualInput", manualInput],
    ["manualCount", manualCount],
    ["manualApplyButton", manualApplyButton],
    ["result", result],
    ["resultPanel", resultPanel],
  ]
    .filter(([, element]) => !element)
    .map(([name]) => name);

  if (missingElements.length > 0) {
    throw new Error(`Missing required app elements: ${missingElements.join(", ")}`);
  }
}

function bindEvents() {
  previousPageButton.addEventListener("click", handlePreviousPage);
  nextPageButton.addEventListener("click", handleNextPage);
  catalogGrid.addEventListener("click", handleCatalogAction);
  selectionList.addEventListener("click", handleSelectionAction);
  selectionToggleButton.addEventListener("click", handleSelectionToggle);
  generateButton.addEventListener("click", generatePdfFromSelection);
  clearButton.addEventListener("click", clearSelection);
  manualToggleButton.addEventListener("click", handleManualToggle);
  manualInput.addEventListener("focus", handleManualFocus);
  manualInput.addEventListener("input", handleManualInput);
  manualApplyButton.addEventListener("click", applyManualSelection);
}

function handleSelectionToggle() {
  if (state.activeSidebarPanel === "selection") {
    showManualPanel();
    renderApp();
    manualInput.focus();
    return;
  }

  showSelectionPanel();
  renderApp();
}

function handleManualToggle() {
  if (state.activeSidebarPanel === "manual") {
    showSelectionPanel();
    renderApp();
    return;
  }

  showManualPanel();
  renderApp();
  manualInput.focus();
}

function handleManualFocus() {
  showManualPanel();
  renderSidebarPanels();
  renderManualControls();
}

function handleManualInput() {
  showManualPanel();
  renderSidebarPanels();
  renderManualControls();
}

async function loadInitialCards() {
  resetCatalog();
  await loadCatalogPage({ pageIndex: 0 });
}

function showSelectionPanel() {
  state.activeSidebarPanel = "selection";
}

function showManualPanel() {
  state.activeSidebarPanel = "manual";
}

function resetCatalog() {
  state.catalogItems = [];
  state.catalogPages = [];
  state.catalogIndex = new Map();
  state.currentCatalogPage = 0;
  state.hasLoadedCatalog = false;
  state.catalogError = "";
}

async function handlePreviousPage() {
  if (state.isCatalogLoading || state.currentCatalogPage === 0) {
    return;
  }

  showCatalogPage(state.currentCatalogPage - 1);
}

async function handleNextPage() {
  if (state.isCatalogLoading) {
    return;
  }

  const nextPageIndex = state.currentCatalogPage + 1;
  if (hasCatalogPage(nextPageIndex)) {
    showCatalogPage(nextPageIndex);
    return;
  }

  const nextCursor = getNextCatalogCursor();
  if (!nextCursor) {
    if (state.catalogError && !getCurrentCatalogPage()) {
      await loadInitialCards();
    }
    return;
  }

  await loadCatalogPage({ after: nextCursor, pageIndex: nextPageIndex });
}

async function loadCatalogPage({ after = null, pageIndex }) {
  state.isCatalogLoading = true;
  state.catalogError = "";
  renderApp();

  try {
    const page = await requestCards(after);
    const items = rememberCatalogPageItems(page.items || []);

    state.catalogPages[pageIndex] = {
      items,
      nextCursor: page.next_cursor || null,
    };
    state.currentCatalogPage = pageIndex;
    state.catalogItems = items;
    state.hasLoadedCatalog = true;
  } catch (error) {
    console.error(error);
    state.catalogError = error.message || "Не удалось загрузить каталог.";
  } finally {
    state.isCatalogLoading = false;
    renderApp();
  }
}

async function requestCards(after) {
  const params = new URLSearchParams({ limit: String(PAGE_SIZE) });
  if (after) {
    params.set("after", after);
  }

  const res = await fetch(`/cards?${params.toString()}`);

  if (!res.ok) {
    const message = (await res.text()).trim();
    throw new Error(message || "Не удалось загрузить каталог.");
  }

  return res.json();
}

function rememberCatalogPageItems(items) {
  const pageItems = [];

  for (const item of items) {
    const nextItem = {
      cardId: item.card_id,
      title: item.title,
      previewUrl: item.preview_url || null,
    };

    const existingItem = state.catalogIndex.get(nextItem.cardId);
    if (existingItem) {
      existingItem.title = nextItem.title;
      existingItem.previewUrl = nextItem.previewUrl;
      pageItems.push(existingItem);
    } else {
      state.catalogIndex.set(nextItem.cardId, nextItem);
      pageItems.push(nextItem);
    }

    syncSelectedCardDetails(nextItem);
  }

  return pageItems;
}

function syncSelectedCardDetails(card) {
  const selectedItem = state.selection.get(card.cardId);
  if (!selectedItem) {
    return;
  }

  selectedItem.title = card.title;
  selectedItem.previewUrl = card.previewUrl;
}

function showCatalogPage(pageIndex) {
  const page = state.catalogPages[pageIndex];
  if (!page) {
    return;
  }

  state.currentCatalogPage = pageIndex;
  state.catalogItems = page.items;
  state.catalogError = "";
  state.hasLoadedCatalog = true;
  renderApp();
}

function getCurrentCatalogPage() {
  return state.catalogPages[state.currentCatalogPage] || null;
}

function getNextCatalogCursor() {
  return getCurrentCatalogPage()?.nextCursor || null;
}

function hasCatalogPage(pageIndex) {
  return Boolean(state.catalogPages[pageIndex]);
}

function readQuantityAction(event) {
  const button = event.target.closest("button[data-action][data-card-id]");
  if (!button) {
    return null;
  }

  const { action, cardId } = button.dataset;
  if (!action || !cardId) {
    return null;
  }

  return { action, cardId };
}

function handleCatalogAction(event) {
  if (state.isGenerating) {
    return;
  }

  const quantityAction = readQuantityAction(event);
  if (!quantityAction) {
    return;
  }

  const { action, cardId } = quantityAction;
  const card = state.catalogIndex.get(cardId);

  if (!card) {
    return;
  }

  showSelectionPanel();

  if (action === "increase") {
    increaseCard(card);
    return;
  }

  if (action === "decrease") {
    decreaseCard(cardId);
  }
}

function handleSelectionAction(event) {
  if (state.isGenerating) {
    return;
  }

  const quantityAction = readQuantityAction(event);
  if (!quantityAction) {
    return;
  }

  const { action, cardId } = quantityAction;
  const card = state.catalogIndex.get(cardId) || state.selection.get(cardId);

  showSelectionPanel();

  if (action === "increase" && card) {
    increaseCard(card);
    return;
  }

  if (action === "decrease") {
    decreaseCard(cardId);
  }
}

function increaseCard(card) {
  const existing = state.selection.get(card.cardId);

  if (existing) {
    existing.count += 1;
  } else {
    state.selection.set(card.cardId, createSelectionItem(card.cardId, card));
  }

  renderApp();
}

function decreaseCard(cardId) {
  const existing = state.selection.get(cardId);
  if (!existing) {
    return;
  }

  if (existing.count <= 1) {
    state.selection.delete(cardId);
  } else {
    existing.count -= 1;
  }

  renderApp();
}

function clearSelection() {
  showSelectionPanel();
  state.selection.clear();
  renderApp();
  setResultState("idle", CLEARED_SELECTION_MESSAGE);
}

async function generatePdfFromSelection() {
  if (state.isGenerating) {
    return;
  }

  const cardIds = flattenSelectionToCardIds();

  if (cardIds.length === 0) {
    setResultState("error", "Выберите хотя бы одну карту.");
    return;
  }

  state.isGenerating = true;
  renderApp();
  setResultState("loading", `Генерация PDF для ${cardIds.length} ID...`);

  try {
    const path = await requestPdf(cardIds);
    showDownloadLink(path, cardIds.length);
  } catch (error) {
    console.error(error);
    setResultState("error", error.message || "Не удалось выполнить запрос.");
  } finally {
    state.isGenerating = false;
    renderApp();
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

function applyManualSelection() {
  const cardIds = getManualCardIds();

  if (cardIds.length === 0) {
    setResultState("error", "Введите хотя бы один ID карты.");
    return;
  }

  showManualPanel();
  state.selection = buildSelection(cardIds);
  renderApp();
  setResultState("idle", `Выбор обновлен по ${cardIds.length} ID.`);
}

function buildSelection(cardIds) {
  const selection = new Map();

  for (const cardId of cardIds) {
    const existing = selection.get(cardId);
    if (existing) {
      existing.count += 1;
      continue;
    }

    selection.set(cardId, createSelectionItem(cardId, state.catalogIndex.get(cardId)));
  }

  return selection;
}

function createSelectionItem(cardId, card) {
  return {
    cardId,
    title: card ? card.title : `Карта ${cardId}`,
    previewUrl: card ? card.previewUrl : null,
    count: 1,
  };
}

function flattenSelectionToCardIds() {
  const cardIds = [];

  for (const item of getSelectedItems()) {
    for (let index = 0; index < item.count; index += 1) {
      cardIds.push(item.cardId);
    }
  }

  return cardIds;
}

function getSelectedItems() {
  return Array.from(state.selection.values()).sort((left, right) =>
    left.cardId.localeCompare(right.cardId, "en", { numeric: true }),
  );
}

function renderApp() {
  renderSidebarPanels();
  renderCatalog();
  renderSelection();
  renderManualControls();
}

function renderSidebarPanels() {
  const selectionIsActive = state.activeSidebarPanel === "selection";
  const manualIsActive = !selectionIsActive;

  selectionPanel.classList.toggle("is-active", selectionIsActive);
  selectionPanel.classList.toggle("is-collapsed", !selectionIsActive);
  manualPanel.classList.toggle("is-active", manualIsActive);
  manualPanel.classList.toggle("is-collapsed", !manualIsActive);

  selectionToggleButton.textContent = selectionIsActive ? "Свернуть" : "Развернуть";
  selectionToggleButton.setAttribute("aria-expanded", String(selectionIsActive));
  manualToggleButton.textContent = manualIsActive ? "Свернуть" : "Развернуть";
  manualToggleButton.setAttribute("aria-expanded", String(manualIsActive));
}

function renderCatalog() {
  renderCatalogStatus();
  renderCatalogControls();
  catalogGrid.replaceChildren();

  if (state.catalogItems.length === 0) {
    return;
  }

  const fragment = document.createDocumentFragment();

  for (const item of state.catalogItems) {
    fragment.append(createCatalogCard(item));
  }

  catalogGrid.append(fragment);
}

function renderCatalogStatus() {
  const statusView = getCatalogStatusView();
  catalogStatus.textContent = statusView.message;
  catalogStatus.className = statusView.className;
}

function renderCatalogControls() {
  const pageNumber = state.currentCatalogPage + 1;
  const hasPreviousPage = state.currentCatalogPage > 0;
  const hasNextPage = hasCatalogPage(state.currentCatalogPage + 1) || Boolean(getNextCatalogCursor());
  const canRetryInitialLoad = state.catalogError && state.catalogItems.length === 0;

  catalogCount.textContent = `Страница ${pageNumber} · ${state.catalogItems.length} карт`;
  pageIndicator.textContent = `Страница ${pageNumber}`;

  if (state.isCatalogLoading) {
    previousPageButton.disabled = true;
    nextPageButton.disabled = true;
    nextPageButton.textContent = "Загружаем...";
    return;
  }

  if (state.catalogError) {
    previousPageButton.disabled = !hasPreviousPage;
    nextPageButton.disabled = !(hasNextPage || canRetryInitialLoad);
    nextPageButton.textContent = "Повторить";
    return;
  }

  previousPageButton.disabled = !hasPreviousPage;
  nextPageButton.disabled = !hasNextPage;
  nextPageButton.textContent = hasNextPage ? "Вперед" : "Последняя страница";
}

function renderSelection() {
  const items = getSelectedItems();
  const totalCopies = items.reduce((sum, item) => sum + item.count, 0);

  selectionBadge.textContent = formatCopies(totalCopies);
  selectionUniqueCount.textContent = String(items.length);
  selectionTotalCount.textContent = String(totalCopies);
  selectionEmpty.classList.toggle("is-hidden", items.length > 0);
  selectionList.replaceChildren();

  const fragment = document.createDocumentFragment();
  for (const item of items) {
    fragment.append(createSelectionRow(item));
  }
  selectionList.append(fragment);

  generateButton.disabled = items.length === 0 || state.isGenerating;
  clearButton.disabled = items.length === 0 || state.isGenerating;
  generateButton.textContent = state.isGenerating ? "Формируем..." : "Сформировать PDF";
}

function createCatalogCard(item) {
  const count = state.selection.get(item.cardId)?.count || 0;
  const card = document.createElement("article");
  card.className = "card-tile";

  const preview = createPreviewFigure(item.previewUrl, item.title, "card-preview");
  appendCopyBadge(preview, count);

  const body = document.createElement("div");
  body.className = "card-body";
  body.append(
    createCardTitle("card-title", item.title),
    createCardMeta("card-meta", item.cardId),
    createQuantityControl(item.cardId, count),
  );

  card.append(preview, body);
  return card;
}

function createSelectionRow(item) {
  const row = document.createElement("article");
  row.className = "selection-row";

  const copy = document.createElement("div");
  copy.className = "selection-copy";
  copy.append(
    createCardTitle("selection-title", item.title),
    createCardMeta("selection-meta", item.cardId),
  );

  row.append(
    createPreviewFigure(item.previewUrl, item.title, "selection-preview"),
    copy,
    createQuantityControl(item.cardId, item.count, true),
  );
  return row;
}

function appendCopyBadge(preview, count) {
  if (count === 0) {
    return;
  }

  const badge = document.createElement("span");
  badge.className = "card-copy-badge";
  badge.textContent = `${count}x`;
  preview.append(badge);
}

function createCardTitle(className, text) {
  const title = document.createElement("h3");
  title.className = className;
  title.textContent = text;
  return title;
}

function createCardMeta(className, cardId) {
  const meta = document.createElement("p");
  meta.className = className;
  meta.textContent = `ID ${cardId}`;
  return meta;
}

function createQuantityControl(cardId, count, compact = false) {
  const control = document.createElement("div");
  control.className = compact ? "quantity-control quantity-control-compact" : "quantity-control";

  const decreaseButton = document.createElement("button");
  decreaseButton.type = "button";
  decreaseButton.className = "icon-button";
  decreaseButton.textContent = "-";
  decreaseButton.dataset.action = "decrease";
  decreaseButton.dataset.cardId = cardId;
  decreaseButton.disabled = count === 0 || state.isGenerating;
  decreaseButton.setAttribute("aria-label", `Уменьшить количество карты ${cardId}`);

  const value = document.createElement("span");
  value.className = "quantity-value";
  value.textContent = String(count);

  const increaseButton = document.createElement("button");
  increaseButton.type = "button";
  increaseButton.className = "icon-button";
  increaseButton.textContent = "+";
  increaseButton.dataset.action = "increase";
  increaseButton.dataset.cardId = cardId;
  increaseButton.disabled = state.isGenerating;
  increaseButton.setAttribute("aria-label", `Увеличить количество карты ${cardId}`);

  control.append(decreaseButton, value, increaseButton);
  return control;
}

function createPreviewFigure(previewUrl, title, className) {
  const figure = document.createElement("div");
  figure.className = className;

  if (previewUrl) {
    const image = document.createElement("img");
    image.src = previewUrl;
    image.alt = title;
    image.loading = "lazy";
    figure.append(image);
    return figure;
  }

  const placeholder = document.createElement("div");
  placeholder.className = "preview-placeholder";
  placeholder.textContent = "Нет превью";
  figure.append(placeholder);
  return figure;
}

function collectCardIds(text) {
  return text
    .split(/[\s,;]+/)
    .map((part) => part.trim())
    .filter(Boolean);
}

function normalizeCardId(cardId) {
  const normalized = String(cardId || "").trim();
  return normalized ? normalized.padStart(3, "0") : "";
}

function getManualCardIds() {
  return collectCardIds(manualInput.value).map(normalizeCardId).filter(Boolean);
}

function renderManualControls() {
  const cardIds = getManualCardIds();
  manualCount.textContent = `${cardIds.length} ID`;
  manualApplyButton.disabled = state.isGenerating || cardIds.length === 0;
}

function getCatalogStatusView() {
  if (state.catalogError) {
    return {
      message: state.catalogError,
      className: "panel-note panel-note-error",
    };
  }

  if (state.isCatalogLoading && state.catalogItems.length === 0) {
    return {
      message: "Загружаем каталог...",
      className: "panel-note",
    };
  }

  if (state.hasLoadedCatalog && state.catalogItems.length === 0) {
    return {
      message: "Каталог пуст.",
      className: "panel-note",
    };
  }

  return {
    message: "",
    className: "panel-note is-hidden",
  };
}

function formatCopies(count) {
  const mod10 = count % 10;
  const mod100 = count % 100;

  if (mod10 === 1 && mod100 !== 11) {
    return `${count} копия`;
  }

  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) {
    return `${count} копии`;
  }

  return `${count} копий`;
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

function setResultState(stateName, message) {
  clearResultState();

  if (stateName === "loading") {
    resultPanel.classList.add("result-loading");
  }
  if (stateName === "error") {
    resultPanel.classList.add("result-error");
  }
  if (stateName === "idle") {
    resultPanel.classList.add("result-idle");
  }

  result.textContent = message;
}

function clearResultState() {
  resultPanel.classList.remove("result-idle", "result-loading", "result-success", "result-error");
}
