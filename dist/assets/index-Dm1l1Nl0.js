(function(){const t=document.createElement("link").relList;if(t&&t.supports&&t.supports("modulepreload"))return;for(const n of document.querySelectorAll('link[rel="modulepreload"]'))l(n);new MutationObserver(n=>{for(const i of n)if(i.type==="childList")for(const y of i.addedNodes)y.tagName==="LINK"&&y.rel==="modulepreload"&&l(y)}).observe(document,{childList:!0,subtree:!0});function a(n){const i={};return n.integrity&&(i.integrity=n.integrity),n.referrerPolicy&&(i.referrerPolicy=n.referrerPolicy),n.crossOrigin==="use-credentials"?i.credentials="include":n.crossOrigin==="anonymous"?i.credentials="omit":i.credentials="same-origin",i}function l(n){if(n.ep)return;n.ep=!0;const i=a(n);fetch(n.href,i)}})();function W(e,t=!1){return window.__TAURI_INTERNALS__.transformCallback(e,t)}async function s(e,t={},a){return window.__TAURI_INTERNALS__.invoke(e,t,a)}var D;(function(e){e.WINDOW_RESIZED="tauri://resize",e.WINDOW_MOVED="tauri://move",e.WINDOW_CLOSE_REQUESTED="tauri://close-requested",e.WINDOW_DESTROYED="tauri://destroyed",e.WINDOW_FOCUS="tauri://focus",e.WINDOW_BLUR="tauri://blur",e.WINDOW_SCALE_FACTOR_CHANGED="tauri://scale-change",e.WINDOW_THEME_CHANGED="tauri://theme-changed",e.WINDOW_CREATED="tauri://window-created",e.WINDOW_SUSPENDED="tauri://suspended",e.WINDOW_RESUMED="tauri://resumed",e.WEBVIEW_CREATED="tauri://webview-created",e.DRAG_ENTER="tauri://drag-enter",e.DRAG_OVER="tauri://drag-over",e.DRAG_DROP="tauri://drag-drop",e.DRAG_LEAVE="tauri://drag-leave"})(D||(D={}));async function k(e,t){window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener(e,t),await s("plugin:event|unlisten",{event:e,eventId:t})}async function x(e,t,a){var l;const n=(l=void 0)!==null&&l!==void 0?l:{kind:"Any"};return s("plugin:event|listen",{event:e,target:n,handler:W(t)}).then(i=>async()=>k(e,i))}function F(e){if(e!==void 0){if(typeof e=="string")return e;if("ok"in e&&"cancel"in e)return{OkCancelCustom:[e.ok,e.cancel]};if("yes"in e&&"no"in e&&"cancel"in e)return{YesNoCancelCustom:[e.yes,e.no,e.cancel]};if("ok"in e)return{OkCustom:e.ok}}}async function T(e,t){return await s("plugin:dialog|message",{message:e,title:t==null?void 0:t.title,kind:t==null?void 0:t.kind,buttons:F(t==null?void 0:t.buttons)})}async function U(e,t){const a=typeof t=="string"?{title:t}:t;return a&&!a.buttons&&a.okLabel&&(a.buttons={ok:a.okLabel}),T(e,a)}const d={na:"https://us.tamrieltradecentre.com/download/PriceTable",eu:"https://eu.tamrieltradecentre.com/download/PriceTable"},R="mac-ttc-language",P={zh:{loading:"讀取中",sourceLabel:"下載來源",currentUrl:"目前網址",destinationLabel:"目的資料夾",checkingDestination:"確認資料夾中",destinationReady:"已找到資料夾，可以下載",destinationMissing:"找不到資料夾，請先確認 ESO AddOns/TamrielTradeCentre 已存在",sourceNaRegion:"北美",sourceEuRegion:"歐洲",run:"進行下載",runningButton:"執行中",lastStarted:"開始時間",lastFinished:"完成時間",lastSuccess:"上次成功",running:"執行中",idle:"待命",openFolder:"開啟資料夾"},en:{loading:"Loading",sourceLabel:"Download Source",currentUrl:"Current URL",destinationLabel:"Destination Folder",checkingDestination:"Checking Folder",destinationReady:"Folder found. Download is available.",destinationMissing:"Folder not found. Confirm that ESO AddOns/TamrielTradeCentre exists first.",sourceNaRegion:"North America",sourceEuRegion:"Europe",run:"Download",runningButton:"Running",lastStarted:"Start Time",lastFinished:"Finish Time",lastSuccess:"Last Success",running:"Running",idle:"Idle",openFolder:"Open Folder"}},N=document.querySelector("#app");if(!N)throw new Error("App root not found");N.innerHTML=`
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
            <input id="source-na" type="radio" name="source" value="${d.na}" />
            <span class="source-icon" aria-hidden="true">🇺🇸</span>
            <span>
              <strong>NA</strong>
              <small data-i18n="sourceNaRegion">北美</small>
            </span>
          </label>
          <label class="source-option">
            <input id="source-eu" type="radio" name="source" value="${d.eu}" />
            <span class="source-icon" aria-hidden="true">🇪🇺</span>
            <span>
              <strong>EU</strong>
              <small data-i18n="sourceEuRegion">歐洲</small>
            </span>
          </label>
        </div>
        <div class="selected-source">
          <span data-i18n="currentUrl">目前網址</span>
          <code id="selected-source-url">${d.na}</code>
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
        <button id="run-now" type="button" data-i18n="run">進行下載</button>
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
`;const g=Array.from(document.querySelectorAll('input[name="source"]')),M=o("selected-source-url"),A=o("destination-path"),m=o("destination-state"),C=o("status-pill"),z=o("last-started"),B=o("last-finished"),G=o("last-success"),S=o("run-now"),u=o("reveal-folder"),O=Array.from(document.querySelectorAll("[data-language]"));let f=J(),v=!1,E="",_=null,L=null;q();g.forEach(e=>{e.addEventListener("change",()=>{w(),j()})});O.forEach(e=>{e.addEventListener("click",()=>{Z(e.dataset.language==="en"?"en":"zh")})});async function q(){I(),x("job-status-changed",e=>{h(e.payload)}),await H(),await $(),await V()}S.addEventListener("click",async()=>{await Q(S,r("runningButton"),async()=>{h(await s("run_now",{config:Y()}))})});u.addEventListener("click",async()=>{try{await s("reveal_destination")}catch(e){c(e)}});async function H(){try{const e=await s("get_config");K(e.url),A.textContent=e.destinationDir}catch(e){c(e)}}async function V(){try{h(await s("get_status"))}catch(e){c(e)}}async function $(){try{const e=await s("get_destination_status");L=e,v=e.exists,A.textContent=e.path,m.textContent=e.exists?r("destinationReady"):r("destinationMissing"),m.classList.toggle("missing",!e.exists),u.disabled=!e.exists,p()}catch(e){v=!1,p(),c(e)}}function Y(){var e;return{url:b(),destinationDir:((e=A.textContent)==null?void 0:e.trim())??""}}function K(e){const t=g.find(a=>a.value===e)??g.find(a=>a.value===d.na);t&&(t.checked=!0),w()}function b(){var e;return((e=g.find(t=>t.checked))==null?void 0:e.value)??d.na}function w(){M.textContent=b()}async function j(){try{await s("set_download_source",{url:b()})}catch(e){c(e)}}function h(e){_=e,C.textContent=e.running?r("running"):r("idle"),C.classList.toggle("running",e.running),z.textContent=e.lastStartedAt??"-",B.textContent=e.lastFinishedAt??"-",G.textContent=e.lastSuccessAt??"-",p(e.running),e.lastError?c(e.lastError):E=""}function p(e=!1){S.disabled=e||!v}async function Q(e,t,a){const l=e.textContent??"";e.disabled=!0,e.textContent=t;try{await a()}catch(n){c(n)}finally{e.textContent=l,p()}}async function c(e){const t=e instanceof Error?e.message:String(e);!t||t===E||(E=t,await U(t,{title:"MacTTC",kind:"error"}))}function Z(e){f=e,ee(e),I(),w(),L&&(m.textContent=L.exists?r("destinationReady"):r("destinationMissing")),_&&h(_)}function I(){document.documentElement.lang=f==="zh"?"zh-Hant":"en",document.querySelectorAll("[data-i18n]").forEach(e=>{const t=e.dataset.i18n;e.textContent=r(t)}),document.querySelectorAll("[data-i18n-aria-label]").forEach(e=>{const t=e.dataset.i18nAriaLabel;e.setAttribute("aria-label",r(t))}),O.forEach(e=>{e.classList.toggle("active",e.dataset.language===f)}),u.title=r("openFolder"),u.setAttribute("aria-label",u.title)}function r(e){return P[f][e]}function J(){return X()??te()}function X(){try{return ne(localStorage.getItem(R))}catch{return null}}function ee(e){try{localStorage.setItem(R,e)}catch{}}function te(){return(navigator.languages[0]??navigator.language).toLowerCase().startsWith("zh")?"zh":"en"}function ne(e){return e==="zh"||e==="en"?e:null}function o(e){const t=document.getElementById(e);if(!t)throw new Error(`Missing element #${e}`);return t}
