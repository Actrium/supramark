import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';

const dom = new JSDOM('<!DOCTYPE html><html><body><div id="container"></div></body></html>', { pretendToBeVisual: true });
const W = dom.window;
for (const k of ['window','document','navigator','HTMLElement','SVGElement','Element','Node','DOMParser','XMLSerializer','getComputedStyle','screen','location','requestAnimationFrame','cancelAnimationFrame','MutationObserver','Image','CSS']) { if (W[k]!==undefined) globalThis[k]=W[k]; }
if (!globalThis.screen) globalThis.screen={availWidth:1024,availHeight:768,width:1024,height:768}; if (!globalThis.CSS) globalThis.CSS={}; if (!globalThis.CSS.supports) globalThis.CSS.supports=()=>true; if (!globalThis.Option){globalThis.Option=function(t='',v=''){const el=W.document.createElement('option');el.text=t;el.value=v;return el;};}
const __ogcs=W.getComputedStyle.bind(W); W.getComputedStyle=function(el,p){const cs=__ogcs(el,p);if(!cs||cs.__ps)return cs;const o=cs.getPropertyValue.bind(cs);cs.getPropertyValue=(n)=>{const v=o(n);return v===''||v==null?'0px':v};try{Object.defineProperty(cs,'__ps',{value:true})}catch{};return cs;}; globalThis.getComputedStyle=W.getComputedStyle;
if(W.HTMLCanvasElement){const noop=()=>{};const mt=()=>({width:0});const mc=()=>({canvas:{width:0,height:0},fillStyle:'',strokeStyle:'',lineWidth:1,font:'',textAlign:'start',textBaseline:'alphabetic',globalAlpha:1,save:noop,restore:noop,translate:noop,rotate:noop,scale:noop,setTransform:noop,beginPath:noop,closePath:noop,moveTo:noop,lineTo:noop,arc:noop,rect:noop,fill:noop,stroke:noop,clip:noop,fillText:noop,strokeText:noop,fillRect:noop,strokeRect:noop,clearRect:noop,measureText:mt,createLinearGradient:()=>({addColorStop:noop}),createRadialGradient:()=>({addColorStop:noop}),drawImage:noop,getImageData:()=>({data:new Uint8ClampedArray(0)}),putImageData:noop});W.HTMLCanvasElement.prototype.getContext=function(){return mc()};}
globalThis.fetch=async()=>({ok:true,status:200,text:async()=>''});
if(W.HTMLImageElement){Object.defineProperty(W.HTMLImageElement.prototype,'src',{configurable:true,set(_v){setTimeout(()=>{this.dispatchEvent&&this.dispatchEvent(new W.Event('error'))},0)},get(){return ''}});Object.defineProperty(W.HTMLImageElement.prototype,'complete',{configurable:true,get(){return true}});if(typeof W.HTMLImageElement.prototype.decode!=='function'){W.HTMLImageElement.prototype.decode=function(){return Promise.resolve()};}}
function resolveFont(el){return{size:14,family:'sans-serif',bold:false};} function intrinsicBox(el){return{x:0,y:0,width:0,height:0};} function elementBBox(){return intrinsicBox(this);} function textLenFn(){const{size,family,bold}=resolveFont(this);return(this.textContent??'').length*8;}
W.SVGElement.prototype.getBBox=elementBBox;W.SVGElement.prototype.getComputedTextLength=textLenFn;if(W.HTMLElement)W.HTMLElement.prototype.getBBox=elementBBox;if(W.Element&&!W.Element.prototype.getBBox)W.Element.prototype.getBBox=elementBBox;function bcr(){return{x:0,y:0,width:0,height:0,top:0,left:0,right:0,bottom:0,toJSON(){return this}};}if(W.HTMLElement)W.HTMLElement.prototype.getBoundingClientRect=bcr;if(W.Element)W.Element.prototype.getBoundingClientRect=bcr;
let __rngState=0x12345678;function __mulberry32(){__rngState=(__rngState+0x6d2b79f5)|0;let t=__rngState;t=Math.imul(t^(t>>>15),1|t);t=(t+Math.imul(t^(t>>>7),61|t))^t;return((t^(t>>>14))>>>0)/4294967296;}Math.random=__mulberry32;W.Math.random=__mulberry32;const FROZEN=Date.parse('2024-01-01T00:00:00Z');Date.now=()=>FROZEN;W.Date.now=()=>FROZEN;

const mermaid=(await import('mermaid')).default;
mermaid.initialize({startOnLoad:false,securityLevel:'loose',handDrawnSeed:1});

// Patch the EXACT graphlib used by mermaid (the bundled one, not the separate dagre-d3-es)
// We need to patch Graph.prototype.children
// The mermaid bundle inlines its own graphlib, so we need to find it

// Strategy: intercept ALL graph.children() calls by patching the method on the 
// actual Graph prototype used by the mermaid bundle

// We can do this by importing the bundle's dagre module
const dagreChunk = await import('/ext/mermaid/tests/support/node_modules/mermaid/dist/chunks/mermaid.core/dagre-KV5264BT.mjs');

// The chunk exports graphlib as a dependency
// Let's try finding the Graph class by checking its prototype
// Actually, let's just search for it by looking at what the chunk exports
console.log('Dagre chunk exports:', Object.keys(dagreChunk));

