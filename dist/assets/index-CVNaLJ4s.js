(function(){const t=document.createElement("link").relList;if(t&&t.supports&&t.supports("modulepreload"))return;for(const n of document.querySelectorAll('link[rel="modulepreload"]'))l(n);new MutationObserver(n=>{for(const r of n)if(r.type==="childList")for(const m of r.addedNodes)m.tagName==="LINK"&&m.rel==="modulepreload"&&l(m)}).observe(document,{childList:!0,subtree:!0});function a(n){const r={};return n.integrity&&(r.integrity=n.integrity),n.referrerPolicy&&(r.referrerPolicy=n.referrerPolicy),n.crossOrigin==="use-credentials"?r.credentials="include":n.crossOrigin==="anonymous"?r.credentials="omit":r.credentials="same-origin",r}function l(n){if(n.ep)return;n.ep=!0;const r=a(n);fetch(n.href,r)}})();function W(e,t=!1){return window.__TAURI_INTERNALS__.transformCallback(e,t)}async function o(e,t={},a){return window.__TAURI_INTERNALS__.invoke(e,t,a)}var D;(function(e){e.WINDOW_RESIZED="tauri://resize",e.WINDOW_MOVED="tauri://move",e.WINDOW_CLOSE_REQUESTED="tauri://close-requested",e.WINDOW_DESTROYED="tauri://destroyed",e.WINDOW_FOCUS="tauri://focus",e.WINDOW_BLUR="tauri://blur",e.WINDOW_SCALE_FACTOR_CHANGED="tauri://scale-change",e.WINDOW_THEME_CHANGED="tauri://theme-changed",e.WINDOW_CREATED="tauri://window-created",e.WINDOW_SUSPENDED="tauri://suspended",e.WINDOW_RESUMED="tauri://resumed",e.WEBVIEW_CREATED="tauri://webview-created",e.DRAG_ENTER="tauri://drag-enter",e.DRAG_OVER="tauri://drag-over",e.DRAG_DROP="tauri://drag-drop",e.DRAG_LEAVE="tauri://drag-leave"})(D||(D={}));async function k(e,t){window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener(e,t),await o("plugin:event|unlisten",{event:e,eventId:t})}async function T(e,t,a){var l;const n=(l=void 0)!==null&&l!==void 0?l:{kind:"Any"};return o("plugin:event|listen",{event:e,target:n,handler:W(t)}).then(r=>async()=>k(e,r))}function F(e){if(e!==void 0){if(typeof e=="string")return e;if("ok"in e&&"cancel"in e)return{OkCancelCustom:[e.ok,e.cancel]};if("yes"in e&&"no"in e&&"cancel"in e)return{YesNoCancelCustom:[e.yes,e.no,e.cancel]};if("ok"in e)return{OkCustom:e.ok}}}async function U(e,t){return await o("plugin:dialog|message",{message:e,title:t==null?void 0:t.title,kind:t==null?void 0:t.kind,buttons:F(t==null?void 0:t.buttons)})}async function M(e,t){const a=typeof t=="string"?{title:t}:t;return a&&!a.buttons&&a.okLabel&&(a.buttons={ok:a.okLabel}),U(e,a)}const c={na:"https://us.tamrieltradecentre.com/download/PriceTable",eu:"https://eu.tamrieltradecentre.com/download/PriceTable"},R="mac-ttc-language",P={zh:{loading:"讀取中",sourceLabel:"下載來源",currentUrl:"目前網址",destinationLabel:"目的資料夾",checkingDestination:"確認資料夾中",destinationReady:"已找到資料夾，可以下載",destinationMissing:"找不到資料夾，請先確認 ESO AddOns/TamrielTradeCentre 已存在",sourceNaRegion:"北美",sourceEuRegion:"歐洲",run:"執行",runningButton:"執行中",status:"狀態",lastStarted:"開始時間",lastFinished:"完成時間",lastSuccess:"上次成功",running:"執行中",idle:"待命",needsAttention:"需要處理",notRunYet:"尚未執行",downloadingArchive:"正在下載壓縮檔",extractingArchive:"下載完成，正在解壓縮",completed:"完成下載與解壓縮",failed:"執行失敗",openFolder:"開啟資料夾"},en:{loading:"Loading",sourceLabel:"Download Source",currentUrl:"Current URL",destinationLabel:"Destination Folder",checkingDestination:"Checking Folder",destinationReady:"Folder found. Download is available.",destinationMissing:"Folder not found. Confirm that ESO AddOns/TamrielTradeCentre exists first.",sourceNaRegion:"North America",sourceEuRegion:"Europe",run:"Run",runningButton:"Running",status:"Status",lastStarted:"Start Time",lastFinished:"Finish Time",lastSuccess:"Last Success",running:"Running",idle:"Idle",needsAttention:"Needs attention",notRunYet:"Not run yet",downloadingArchive:"Downloading archive",extractingArchive:"Download complete. Extracting archive",completed:"Download and extraction complete",failed:"Execution failed",openFolder:"Open Folder"}},G={讀取中:"loading",尚未執行:"notRunYet",正在下載壓縮檔:"downloadingArchive","下載完成，正在解壓縮":"extractingArchive",完成下載與解壓縮:"completed",執行失敗:"failed"},N=document.querySelector("#app");if(!N)throw new Error("App root not found");N.innerHTML=`
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
            <input id="source-na" type="radio" name="source" value="${c.na}" />
            <span class="source-icon" aria-hidden="true">🇺🇸</span>
            <span>
              <strong>NA</strong>
              <small data-i18n="sourceNaRegion">北美</small>
            </span>
          </label>
          <label class="source-option">
            <input id="source-eu" type="radio" name="source" value="${c.eu}" />
            <span class="source-icon" aria-hidden="true">🇪🇺</span>
            <span>
              <strong>EU</strong>
              <small data-i18n="sourceEuRegion">歐洲</small>
            </span>
          </label>
        </div>
        <div class="selected-source">
          <span data-i18n="currentUrl">目前網址</span>
          <code id="selected-source-url">${c.na}</code>
        </div>
      </div>

      <div class="field">
        <span class="field-label" data-i18n="destinationLabel">目的資料夾</span>
        <div class="destination-display">
          <code id="destination-path">讀取中</code>
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
      <div hidden>
        <p class="eyebrow" data-i18n="status">狀態</p>
        <h2 id="message" data-i18n="loading">讀取中</h2>
      </div>
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
      <p id="update-summary" class="update-summary" hidden></p>
    </section>
  </section>
`;const g=Array.from(document.querySelectorAll('input[name="source"]')),z=s("selected-source-url"),w=s("destination-path"),y=s("destination-state"),C=s("status-pill"),O=s("message"),B=s("last-started"),q=s("last-finished"),Y=s("last-success"),S=s("run-now"),u=s("reveal-folder"),x=Array.from(document.querySelectorAll("[data-language]"));let f=ne(),v=!1,E="",A=null,_=null;H();g.forEach(e=>{e.addEventListener("change",()=>{b(),Z()})});x.forEach(e=>{e.addEventListener("click",()=>{ee(e.dataset.language==="en"?"en":"zh")})});async function H(){I(),T("job-status-changed",e=>{h(e.payload)}),await V(),await K(),await $()}S.addEventListener("click",async()=>{await X(S,i("runningButton"),async()=>{h(await o("run_now",{config:j()}))})});u.addEventListener("click",async()=>{try{await o("reveal_destination")}catch(e){d(e)}});async function V(){try{const e=await o("get_config");Q(e.url),w.textContent=e.destinationDir}catch(e){d(e)}}async function $(){try{h(await o("get_status"))}catch(e){d(e)}}async function K(){try{const e=await o("get_destination_status");_=e,v=e.exists,w.textContent=e.path,y.textContent=e.exists?i("destinationReady"):i("destinationMissing"),y.classList.toggle("missing",!e.exists),u.disabled=!e.exists,p()}catch(e){v=!1,p(),d(e)}}function j(){var e;return{url:L(),destinationDir:((e=w.textContent)==null?void 0:e.trim())??""}}function Q(e){const t=g.find(a=>a.value===e)??g.find(a=>a.value===c.na);t&&(t.checked=!0),b()}function L(){var e;return((e=g.find(t=>t.checked))==null?void 0:e.value)??c.na}function b(){z.textContent=L()}async function Z(){try{await o("set_download_source",{url:L()})}catch(e){d(e)}}function h(e){A=e,C.textContent=e.running?i("running"):i("idle"),C.classList.toggle("running",e.running),O.textContent=te(e),B.textContent=e.lastStartedAt??"-",q.textContent=e.lastFinishedAt??"-",Y.textContent=e.lastSuccessAt??"-",J(),p(e.running),e.lastError?d(e.lastError):E=""}function J(e){const t=s("update-summary");t.hidden=!0,t.textContent=""}function p(e=!1){S.disabled=e||!v}async function X(e,t,a){const l=e.textContent??"";e.disabled=!0,e.textContent=t;try{await a()}catch(n){d(n)}finally{e.textContent=l,p()}}async function d(e){const t=e instanceof Error?e.message:String(e);!t||t===E||(E=t,O.textContent=i("needsAttention"),await M(t,{title:"MacTTC",kind:"error"}))}function ee(e){f=e,ie(e),I(),b(),_&&(y.textContent=_.exists?i("destinationReady"):i("destinationMissing")),A&&h(A)}function I(){document.documentElement.lang=f==="zh"?"zh-Hant":"en",document.querySelectorAll("[data-i18n]").forEach(e=>{const t=e.dataset.i18n;e.textContent=i(t)}),document.querySelectorAll("[data-i18n-aria-label]").forEach(e=>{const t=e.dataset.i18nAriaLabel;e.setAttribute("aria-label",i(t))}),x.forEach(e=>{e.classList.toggle("active",e.dataset.language===f)}),u.title=i("openFolder"),u.setAttribute("aria-label",u.title)}function te(e){if(!e.message)return"";if(e.lastSuccessAt&&!e.lastError)return i("completed");const t=G[e.message];return t?i(t):e.running?i("running"):e.message}function i(e){return P[f][e]}function ne(){return ae()??re()}function ae(){try{return se(localStorage.getItem(R))}catch{return null}}function ie(e){try{localStorage.setItem(R,e)}catch{}}function re(){return(navigator.languages[0]??navigator.language).toLowerCase().startsWith("zh")?"zh":"en"}function se(e){return e==="zh"||e==="en"?e:null}function s(e){const t=document.getElementById(e);if(!t)throw new Error(`Missing element #${e}`);return t}
