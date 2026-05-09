import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';

const dom = new JSDOM('<!DOCTYPE html><html><body><div id="container"></div></body></html>', { pretendToBeVisual: true });
const W = dom.window;
for (const k of ['window','document','navigator','HTMLElement','SVGElement','Element','Node','DOMParser','XMLSerializer','getComputedStyle','screen','location','requestAnimationFrame','cancelAnimationFrame','MutationObserver','Image','CSS']) {
  if (W[k] !== undefined) globalThis[k] = W[k];
}
if (!globalThis.screen) globalThis.screen = { availWidth: 1024, availHeight: 768, width: 1024, height: 768 };
if (!globalThis.CSS) globalThis.CSS = {};
if (!globalThis.CSS.supports) globalThis.CSS.supports = () => true;
if (!globalThis.Option) { globalThis.Option = function(t='',v='') { const el = W.document.createElement('option'); el.text=t; el.value=v; return el; }; }
const __origGetComputedStyle = W.getComputedStyle.bind(W);
W.getComputedStyle = function(el, pseudo) {
  const cs = __origGetComputedStyle(el, pseudo);
  if (!cs || cs.__paddingShimmed) return cs;
  const origGet = cs.getPropertyValue.bind(cs);
  cs.getPropertyValue = (name) => { const v = origGet(name); return v === '' || v == null ? '0px' : v; };
  try { Object.defineProperty(cs, '__paddingShimmed', { value: true }); } catch {}
  return cs;
};
globalThis.getComputedStyle = W.getComputedStyle;
if (W.HTMLCanvasElement) {
  const noop=()=>{}; const mt=()=>({width:0}); const mc=()=>({canvas:{width:0,height:0},fillStyle:'',strokeStyle:'',lineWidth:1,font:'',textAlign:'start',textBaseline:'alphabetic',globalAlpha:1,save:noop,restore:noop,translate:noop,rotate:noop,scale:noop,setTransform:noop,beginPath:noop,closePath:noop,moveTo:noop,lineTo:noop,arc:noop,rect:noop,fill:noop,stroke:noop,clip:noop,fillText:noop,strokeText:noop,fillRect:noop,strokeRect:noop,clearRect:noop,measureText:mt,createLinearGradient:()=>({addColorStop:noop}),createRadialGradient:()=>({addColorStop:noop}),drawImage:noop,getImageData:()=>({data:new Uint8ClampedArray(0)}),putImageData:noop});
  W.HTMLCanvasElement.prototype.getContext = function(){return mc()};
}
globalThis.fetch = async () => ({ ok: true, status: 200, text: async () => '', json: async () => ({}) });
if (W.HTMLImageElement) {
  Object.defineProperty(W.HTMLImageElement.prototype, 'src', { configurable:true, set(_v){setTimeout(()=>{this.dispatchEvent&&this.dispatchEvent(new W.Event('error'))},0)}, get(){return ''} });
  Object.defineProperty(W.HTMLImageElement.prototype, 'complete', { configurable:true, get(){return true} });
  if (typeof W.HTMLImageElement.prototype.decode !== 'function') { W.HTMLImageElement.prototype.decode = function(){return Promise.resolve()}; }
}

// Font shims
function resolveFont(el){return{size:14,family:'sans-serif',bold:false};}
function textWidthFn(t,f,s,b){return t.length*8;}
function measureTextBlockFn(t,f,s,b){return{width:t.length*8,height:14};}
function intrinsicBox(el){return{x:0,y:0,width:0,height:0};}
function elementBBox(){return intrinsicBox(this);}
function textLenFn(){const{size,family,bold}=resolveFont(this);return textWidthFn(this.textContent??'',family,size,bold);}
W.SVGElement.prototype.getBBox=elementBBox; W.SVGElement.prototype.getComputedTextLength=textLenFn;
if(W.HTMLElement)W.HTMLElement.prototype.getBBox=elementBBox;
if(W.Element&&!W.Element.prototype.getBBox)W.Element.prototype.getBBox=elementBBox;
function bcr(){return{x:0,y:0,width:0,height:0,top:0,left:0,right:0,bottom:0,toJSON(){return this}};}
if(W.HTMLElement)W.HTMLElement.prototype.getBoundingClientRect=bcr;
if(W.Element)W.Element.prototype.getBoundingClientRect=bcr;

let __rngState=0x12345678;
function __mulberry32(){__rngState=(__rngState+0x6d2b79f5)|0;let t=__rngState;t=Math.imul(t^(t>>>15),1|t);t=(t+Math.imul(t^(t>>>7),61|t))^t;return((t^(t>>>14))>>>0)/4294967296;}
Math.random=__mulberry32; W.Math.random=__mulberry32;
const FROZEN=Date.parse('2024-01-01T00:00:00Z'); Date.now=()=>FROZEN; W.Date.now=()=>FROZEN;

const mermaid = (await import('mermaid')).default;
mermaid.initialize({ startOnLoad: false, securityLevel: 'loose', handDrawnSeed: 1 });

// Instrument: track when <g class="node"> elements are inserted into the DOM
// by monkey-patching Element.prototype.insertBefore
const nodeInsertLog = [];
const origInsertBefore = W.Element.prototype.insertBefore;
W.Element.prototype.insertBefore = function(newNode, refNode) {
  if (newNode && newNode.getAttribute && newNode.getAttribute('class') && 
      newNode.getAttribute('class').includes('node') && 
      newNode.getAttribute('id') && newNode.getAttribute('id').includes('flowchart-')) {
    nodeInsertLog.push({
      id: newNode.getAttribute('id'),
      parentId: this.getAttribute ? (this.getAttribute('id') || this.getAttribute('class') || 'unknown') : 'unknown',
      refNodeClass: refNode ? (refNode.getAttribute ? refNode.getAttribute('class') : null) : null,
      timestamp: Date.now()
    });
  }
  return origInsertBefore.call(this, newNode, refNode);
};
globalThis.Element.prototype.insertBefore = W.Element.prototype.insertBefore;

const source = readFileSync('/ext/mermaid/tests/ext_fixtures/cypress/flowchart/134.mmd', 'utf8');
__rngState = 0x12345678;
const { svg } = await mermaid.render('ref-ext-fixtures-cypress-flowchart-134', source);

console.log('\nNode insertion log (order of insertBefore calls):');
for (const entry of nodeInsertLog) {
  console.log(`  ${entry.id} -> parent=${entry.parentId} refNode=${entry.refNodeClass}`);
}
