import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { message as showDialogMessage } from "@tauri-apps/plugin-dialog";
import "./styles.css";

type AppConfig = {
  url: string;
  destinationDir: string;
};

type JobStatus = {
  running: boolean;
  lastStartedAt: string | null;
  lastFinishedAt: string | null;
  lastSuccessAt: string | null;
  lastError: string | null;
  sourceUrl: string | null;
  destinationDir: string | null;
  message: string;
};

type DestinationStatus = {
  path: string;
  exists: boolean;
};

type Language = "zh" | "en";

type TranslationKey =
  | "loading"
  | "sourceLabel"
  | "currentUrl"
  | "destinationLabel"
  | "checkingDestination"
  | "destinationReady"
  | "destinationMissing"
  | "sourceNaRegion"
  | "sourceEuRegion"
  | "run"
  | "runningButton"
  | "lastStarted"
  | "lastFinished"
  | "lastSuccess"
  | "running"
  | "idle"
  | "openFolder";

const DOWNLOAD_SOURCES = {
  na: "https://us.tamrieltradecentre.com/download/PriceTable",
  eu: "https://eu.tamrieltradecentre.com/download/PriceTable",
} as const;

const LANGUAGE_STORAGE_KEY = "mac-ttc-language";

const TRANSLATIONS: Record<Language, Record<TranslationKey, string>> = {
  zh: {
    loading: "讀取中",
    sourceLabel: "下載來源",
    currentUrl: "目前網址",
    destinationLabel: "目的資料夾",
    checkingDestination: "確認資料夾中",
    destinationReady: "已找到資料夾，可以下載",
    destinationMissing:
      "找不到資料夾，請先確認 ESO AddOns/TamrielTradeCentre 已存在",
    sourceNaRegion: "北美",
    sourceEuRegion: "歐洲",
    run: "執行",
    runningButton: "執行中",
    lastStarted: "開始時間",
    lastFinished: "完成時間",
    lastSuccess: "上次成功",
    running: "執行中",
    idle: "待命",
    openFolder: "開啟資料夾",
  },
  en: {
    loading: "Loading",
    sourceLabel: "Download Source",
    currentUrl: "Current URL",
    destinationLabel: "Destination Folder",
    checkingDestination: "Checking Folder",
    destinationReady: "Folder found. Download is available.",
    destinationMissing:
      "Folder not found. Confirm that ESO AddOns/TamrielTradeCentre exists first.",
    sourceNaRegion: "North America",
    sourceEuRegion: "Europe",
    run: "Run",
    runningButton: "Running",
    lastStarted: "Start Time",
    lastFinished: "Finish Time",
    lastSuccess: "Last Success",
    running: "Running",
    idle: "Idle",
    openFolder: "Open Folder",
  },
};

const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
  throw new Error("App root not found");
}

app.innerHTML = `
  <section class="shell">
    <header class="topbar">
      <div>
        <h1>MacTTC</h1>
      </div>
      <div class="topbar-actions">
        <div class="language-toggle" role="group" aria-label="Language">
          <button id="lang-zh" type="button" class="language-button active" data-language="zh">中文</button>
          <button id="lang-en" type="button" class="language-button" data-language="en">English</button>
        </div>
        <div class="status-pill" id="status-pill" data-i18n="loading">讀取中</div>
      </div>
    </header>

    <section class="panel">
      <div class="field">
        <span class="field-label" data-i18n="sourceLabel">下載來源</span>
        <div class="source-options" role="radiogroup" aria-label="下載來源" data-i18n-aria-label="sourceLabel">
          <label class="source-option">
            <input id="source-na" type="radio" name="source" value="${DOWNLOAD_SOURCES.na}" />
            <span class="source-icon" aria-hidden="true">🇺🇸</span>
            <span>
              <strong>NA</strong>
              <small data-i18n="sourceNaRegion">北美</small>
            </span>
          </label>
          <label class="source-option">
            <input id="source-eu" type="radio" name="source" value="${DOWNLOAD_SOURCES.eu}" />
            <span class="source-icon" aria-hidden="true">🇪🇺</span>
            <span>
              <strong>EU</strong>
              <small data-i18n="sourceEuRegion">歐洲</small>
            </span>
          </label>
        </div>
        <div class="selected-source">
          <span data-i18n="currentUrl">目前網址</span>
          <code id="selected-source-url">${DOWNLOAD_SOURCES.na}</code>
        </div>
      </div>

      <div class="field">
        <div class="destination-row">
          <div class="destination-display">
            <span data-i18n="destinationLabel">目的資料夾</span>
            <code id="destination-path">讀取中</code>
          </div>
          <button id="reveal-folder" class="icon-button" type="button" title="開啟資料夾" aria-label="開啟資料夾">
            <span aria-hidden="true">↗</span>
          </button>
        </div>
        <p id="destination-state" class="path-state" data-i18n="checkingDestination">確認資料夾中</p>
      </div>

      <div class="actions">
        <button id="run-now" type="button" data-i18n="run">執行</button>
      </div>
    </section>

    <section class="status-panel">
      <dl>
        <div>
          <dt data-i18n="lastStarted">開始時間</dt>
          <dd id="last-started">-</dd>
        </div>
        <div>
          <dt data-i18n="lastFinished">完成時間</dt>
          <dd id="last-finished">-</dd>
        </div>
        <div>
          <dt data-i18n="lastSuccess">上次成功</dt>
          <dd id="last-success">-</dd>
        </div>
      </dl>
    </section>
  </section>
`;

const sourceInputs = Array.from(
  document.querySelectorAll<HTMLInputElement>('input[name="source"]'),
);
const selectedSource = getElement<HTMLElement>("selected-source-url");
const destinationPath = getElement<HTMLElement>("destination-path");
const destinationState = getElement<HTMLParagraphElement>("destination-state");
const statusPill = getElement<HTMLDivElement>("status-pill");
const lastStarted = getElement<HTMLElement>("last-started");
const lastFinished = getElement<HTMLElement>("last-finished");
const lastSuccess = getElement<HTMLElement>("last-success");
const runNowButton = getElement<HTMLButtonElement>("run-now");
const revealFolderButton = getElement<HTMLButtonElement>("reveal-folder");
const languageButtons = Array.from(
  document.querySelectorAll<HTMLButtonElement>("[data-language]"),
);

let currentLanguage: Language = initialLanguage();
let destinationAvailable = false;
let lastDialogError = "";
let lastStatus: JobStatus | null = null;
let lastDestinationStatus: DestinationStatus | null = null;

void bootstrap();

sourceInputs.forEach((input) => {
  input.addEventListener("change", () => {
    updateSelectedSourceDisplay();
    void saveSelectedSource();
  });
});

languageButtons.forEach((button) => {
  button.addEventListener("click", () => {
    setLanguage(button.dataset.language === "en" ? "en" : "zh");
  });
});

async function bootstrap() {
  applyTranslations();
  void listen<JobStatus>("job-status-changed", (event) => {
    renderStatus(event.payload);
  });
  await loadConfig();
  await refreshDestinationStatus();
  await refreshStatus();
}

runNowButton.addEventListener("click", async () => {
  await withBusy(runNowButton, t("runningButton"), async () => {
    renderStatus(await invoke<JobStatus>("run_now", { config: readForm() }));
  });
});

revealFolderButton.addEventListener("click", async () => {
  try {
    await invoke("reveal_destination");
  } catch (error) {
    void showError(error);
  }
});

async function loadConfig() {
  try {
    const config = await invoke<AppConfig>("get_config");
    setSelectedSource(config.url);
    destinationPath.textContent = config.destinationDir;
  } catch (error) {
    void showError(error);
  }
}

async function refreshStatus() {
  try {
    renderStatus(await invoke<JobStatus>("get_status"));
  } catch (error) {
    void showError(error);
  }
}

async function refreshDestinationStatus() {
  try {
    const status = await invoke<DestinationStatus>("get_destination_status");
    lastDestinationStatus = status;
    destinationAvailable = status.exists;
    destinationPath.textContent = status.path;
    destinationState.textContent = status.exists
      ? t("destinationReady")
      : t("destinationMissing");
    destinationState.classList.toggle("missing", !status.exists);
    revealFolderButton.disabled = !status.exists;
    updateActionAvailability();
  } catch (error) {
    destinationAvailable = false;
    updateActionAvailability();
    void showError(error);
  }
}

function readForm(): AppConfig {
  return {
    url: selectedSourceUrl(),
    destinationDir: destinationPath.textContent?.trim() ?? "",
  };
}

function setSelectedSource(url: string) {
  const matched =
    sourceInputs.find((input) => input.value === url) ??
    sourceInputs.find((input) => input.value === DOWNLOAD_SOURCES.na);

  if (matched) {
    matched.checked = true;
  }
  updateSelectedSourceDisplay();
}

function selectedSourceUrl(): string {
  return (
    sourceInputs.find((input) => input.checked)?.value ?? DOWNLOAD_SOURCES.na
  );
}

function updateSelectedSourceDisplay() {
  selectedSource.textContent = selectedSourceUrl();
}

async function saveSelectedSource() {
  try {
    await invoke<AppConfig>("set_download_source", {
      url: selectedSourceUrl(),
    });
  } catch (error) {
    void showError(error);
  }
}

function renderStatus(status: JobStatus) {
  lastStatus = status;
  statusPill.textContent = status.running ? t("running") : t("idle");
  statusPill.classList.toggle("running", status.running);
  lastStarted.textContent = status.lastStartedAt ?? "-";
  lastFinished.textContent = status.lastFinishedAt ?? "-";
  lastSuccess.textContent = status.lastSuccessAt ?? "-";
  updateActionAvailability(status.running);

  if (status.lastError) {
    void showError(status.lastError);
  } else {
    lastDialogError = "";
  }
}

function updateActionAvailability(running = false) {
  runNowButton.disabled = running || !destinationAvailable;
}

async function withBusy(
  button: HTMLButtonElement,
  label: string,
  action: () => Promise<void>,
) {
  const original = button.textContent ?? "";
  button.disabled = true;
  button.textContent = label;

  try {
    await action();
  } catch (error) {
    void showError(error);
  } finally {
    button.textContent = original;
    updateActionAvailability();
  }
}

async function showError(error: unknown) {
  const text = error instanceof Error ? error.message : String(error);
  if (!text || text === lastDialogError) {
    return;
  }

  lastDialogError = text;
  await showDialogMessage(text, {
    title: "MacTTC",
    kind: "error",
  });
}

function setLanguage(language: Language) {
  currentLanguage = language;
  saveLanguagePreference(language);
  applyTranslations();
  updateSelectedSourceDisplay();
  if (lastDestinationStatus) {
    destinationState.textContent = lastDestinationStatus.exists
      ? t("destinationReady")
      : t("destinationMissing");
  }
  if (lastStatus) {
    renderStatus(lastStatus);
  }
}

function applyTranslations() {
  document.documentElement.lang = currentLanguage === "zh" ? "zh-Hant" : "en";
  document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((element) => {
    const key = element.dataset.i18n as TranslationKey;
    element.textContent = t(key);
  });
  document
    .querySelectorAll<HTMLElement>("[data-i18n-aria-label]")
    .forEach((element) => {
      const key = element.dataset.i18nAriaLabel as TranslationKey;
      element.setAttribute("aria-label", t(key));
    });
  languageButtons.forEach((button) => {
    button.classList.toggle("active", button.dataset.language === currentLanguage);
  });
  revealFolderButton.title = t("openFolder");
  revealFolderButton.setAttribute("aria-label", revealFolderButton.title);
}

function t(key: TranslationKey): string {
  return TRANSLATIONS[currentLanguage][key];
}

function initialLanguage(): Language {
  return savedLanguagePreference() ?? systemLanguageDefault();
}

function savedLanguagePreference(): Language | null {
  try {
    return parseLanguage(localStorage.getItem(LANGUAGE_STORAGE_KEY));
  } catch {
    return null;
  }
}

function saveLanguagePreference(language: Language) {
  try {
    localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
  } catch {
    // Ignore storage failures; the current session can still switch language.
  }
}

function systemLanguageDefault(): Language {
  const language = navigator.languages[0] ?? navigator.language;
  return language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

function parseLanguage(value: string | null | undefined): Language | null {
  return value === "zh" || value === "en" ? value : null;
}

function getElement<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`Missing element #${id}`);
  }
  return element as T;
}
