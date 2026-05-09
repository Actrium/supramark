import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';
import { resolve, dirname, basename, extname } from 'node:path';
import { fileURLToPath } from 'node:url';

const dom = new JSDOM(`<!DOCTYPE html><html><body><div id="container"></div></body></html>`, { pretendToBeVisual: true });
const W = dom.window;
for (const k of ['window','document','navigator','HTMLElement','SVGElement','Element','Node','DOMParser','XMLSerializer','getComputedStyle','screen','location','requestAnimationFrame','cancelAnimationFrame','MutationObserver','Image','CSS']) {
  if (W[k] !== undefined) globalThis[k] = W[k];
}
if (!globalThis.screen) globalThis.screen = {availWidth:1024,availHeight:768,width:1024,height:768};
const FROZEN = Date.parse('2024-01-01T00:00:00Z');
Date.now = () => FROZEN; W.Date.now = () => FROZEN;
if (!globalThis.CSS) globalThis.CSS = {};
if (!globalThis.CSS.supports) globalThis.CSS.supports = () => true;
if (!W.CSS) W.CSS = globalThis.CSS;
else if (!W.CSS.supports) W.CSS.supports = globalThis.CSS.supports;
if (!globalThis.Option && W.document) { globalThis.Option = function(t='',v='') { const el = W.document.createElement('option'); el.text=t; el.value=v; return el; }; }

import { textWidth, lineHeight, measureTextBlock } from './font_metrics.mjs';
function resolveFont(el) { let size=14,family='sans-serif',bold=false; let node=el; while(node&&node.nodeType===1){ const g=(k)=>(node.getAttribute?node.getAttribute(k):null); const style=node.style??{}; if(!size){const s=g('font-size')??style.fontSize??null;if(s){const m=/([0-9.]+)/.exec(String(s));if(m){size=parseFloat(m[1]);}}} if(!family){const f=g('font-family')??style.fontFamily??null;if(f){family=String(f);}} if(!bold){const w=g('font-weight')??style.fontWeight??null;if(w){const s=String(w).trim().toLowerCase();bold=s==='bold'||s==='bolder'||(parseInt(s,10)>=600);}} node=node.parentNode; } return {size,family,bold}; }
function attrNum(el,k,d=0){const v=el?.getAttribute?el.getAttribute(k):null;if(v==null||v==='')return d;const n=parseFloat(v);return Number.isFinite(n)?n:d;}
const NON_VISIBLE=new Set(['style','defs','metadata','title','desc','script','marker','pattern','mask','clippath','symbol','lineargradient','radialgradient','filter']);
function pathBBox(d){if(!d)return{x:0,y:0,width:0,height:0};let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;let cx=0,cy=0,startX=0,startY=0;const add=(x,y)=>{if(x<minX)minX=x;if(y<minY)minY=y;if(x>maxX)maxX=x;if(y>maxY)maxY=y;};const tok=d.match(/[MmLlHhVvZzCcSsQqTtAa]|-?\d*\.?\d+(?:[eE][-+]?\d+)?/g)??[];let i=0,cmd='';const read=()=>parseFloat(tok[i++]);while(i<tok.length){const t=tok[i];if(/^[A-Za-z]$/.test(t)){cmd=t;i++;}const rel=cmd===cmd.toLowerCase();switch(cmd){case 'M':case 'm':{let x=read(),y=read();if(rel){x+=cx;y+=cy;}cx=x;cy=y;startX=x;startY=y;add(x,y);cmd=rel?'l':'L';break;}case 'L':case 'l':{let x=read(),y=read();if(rel){x+=cx;y+=cy;}cx=x;cy=y;add(x,y);break;}case 'H':case 'h':{let x=read();if(rel)x+=cx;cx=x;add(x,cy);break;}case 'V':case 'v':{let y=read();if(rel)y+=cy;cy=y;add(cx,y);break;}case 'C':case 'c':{let x1=read(),y1=read(),x2=read(),y2=read(),x=read(),y=read();if(rel){x1+=cx;y1+=cy;x2+=cx;y2+=cy;x+=cx;y+=cy;}add(x1,y1);add(x2,y2);add(x,y);cx=x;cy=y;break;}case 'S':case 's':{let x2=read(),y2=read(),x=read(),y=read();if(rel){x2+=cx;y2+=cy;x+=cx;y+=cy;}add(x2,y2);add(x,y);cx=x;cy=y;break;}case 'Q':case 'q':{let x1=read(),y1=read(),x=read(),y=read();if(rel){x1+=cx;y1+=cy;x+=cx;y+=cy;}add(x1,y1);add(x,y);cx=x;cy=y;break;}case 'T':case 't':{let x=read(),y=read();if(rel){x+=cx;y+=cy;}add(x,y);cx=x;cy=y;break;}case 'A':case 'a':{read();read();read();read();read();let x=read(),y=read();if(rel){x+=cx;y+=cy;}add(x,y);cx=x;cy=y;break;}case 'Z':case 'z':cx=startX;cy=startY;break;default:i++;}}if(!Number.isFinite(minX))return{x:0,y:0,width:0,height:0};return{x:minX,y:minY,width:maxX-minX,height:maxY-minY};}
function polyBBox(p){if(!p)return{x:0,y:0,width:0,height:0};const parts=p.trim().split(/[\s,]+/).filter(Boolean).map(parseFloat);let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;for(let i=0;i+1<parts.length;i+=2){const x=parts[i],y=parts[i+1];if(!Number.isFinite(x)||!Number.isFinite(y))continue;if(x<minX)minX=x;if(y<minY)minY=y;if(x>maxX)maxX=x;if(y>maxY)maxY=y;}if(!Number.isFinite(minX))return{x:0,y:0,width:0,height:0};return{x:minX,y:minY,width:maxX-minX,height:maxY-minY};}
function intrinsicBox(el){const tag=(el.tagName??'').toLowerCase();if(NON_VISIBLE.has(tag))return{x:0,y:0,width:0,height:0};if(tag==='rect')return{x:attrNum(el,'x'),y:attrNum(el,'y'),width:attrNum(el,'width'),height:attrNum(el,'height')};if(tag==='circle'){const r=attrNum(el,'r');return{x:attrNum(el,'cx')-r,y:attrNum(el,'cy')-r,width:r*2,height:r*2};}if(tag==='ellipse'){const rx=attrNum(el,'rx'),ry=attrNum(el,'ry');return{x:attrNum(el,'cx')-rx,y:attrNum(el,'cy')-ry,width:rx*2,height:ry*2};}if(tag==='line'){const x1=attrNum(el,'x1'),y1=attrNum(el,'y1'),x2=attrNum(el,'x2'),y2=attrNum(el,'y2');return{x:Math.min(x1,x2),y:Math.min(y1,y2),width:Math.abs(x2-x1),height:Math.abs(y2-y1)};};if(tag==='polygon'||tag==='polyline')return polyBBox(el.getAttribute?.('points')??'');if(tag==='path')return pathBBox(el.getAttribute?.('d')??'');if(tag==='foreignobject'){const w=attrNum(el,'width',-1),h=attrNum(el,'height',-1);if(w>=0&&h>=0)return{x:attrNum(el,'x'),y:attrNum(el,'y'),width:w,height:h};const{size,family,bold}=resolveFont(el);const{width,height}=measureTextBlock(el.textContent??'',family,size,bold);return{x:0,y:0,width,height};}if(tag==='text'||tag==='tspan'){const{size,family,bold}=resolveFont(el);const{width,height}=measureTextBlock(el.textContent??'',family,size,bold);return{x:0,y:0,width,height};}if(tag==='g'||tag==='svg')return null;const{size,family,bold}=resolveFont(el);const{width,height}=measureTextBlock(el.textContent??'',family,size,bold);return{x:0,y:0,width,height};}
function unionBox(boxes){let minX=Infinity,minY=Infinity,maxX=-Infinity,maxY=-Infinity;let found=false;for(const b of boxes){if(!b)continue;if(b.width===0&&b.height===0)continue;found=true;if(b.x<minX)minX=b.x;if(b.y<minY)minY=b.y;if(b.x+b.width>maxX)maxX=b.x+b.width;if(b.y+b.height>maxY)maxY=b.y+b.height;}if(!found)return{x:0,y:0,width:0,height:0};return{x:minX,y:minY,width:maxX-minX,height:maxY-minY};}
function elementBBox(){const intrinsic=intrinsicBox(this);if(intrinsic)return intrinsic;const stack=[this];const boxes=[];let depth=0;while(stack.length&&depth++<5000){const n=stack.pop();for(const c of n.children??[]){const ib=intrinsicBox(c);if(ib)boxes.push(ib);else stack.push(c);}}return unionBox(boxes);}
function textLen(){const{size,family,bold}=resolveFont(this);return textWidth(this.textContent??'',family,size,bold);}
const textBBox=elementBBox;
W.SVGElement.prototype.getBBox=textBBox;W.SVGElement.prototype.getComputedTextLength=textLen;
if(W.HTMLElement)W.HTMLElement.prototype.getBBox=textBBox;
if(W.Element&&!W.Element.prototype.getBBox)W.Element.prototype.getBBox=textBBox;
function boundingClientRectShim(){const b=elementBBox.call(this);return{x:b.x,y:b.y,width:b.width,height:b.height,top:b.y,left:b.x,right:b.x+b.width,bottom:b.y+b.height,toJSON(){return{x:b.x,y:b.y,width:b.width,height:b.height,top:b.y,left:b.x,right:b.x+b.width,bottom:b.y+b.height};}};}
if(W.HTMLElement)W.HTMLElement.prototype.getBoundingClientRect=boundingClientRectShim;
if(W.Element)W.Element.prototype.getBoundingClientRect=boundingClientRectShim;
let __rngState=0x12345678;function __mulberry32(){__rngState=(__rngState+0x6d2b79f5)|0;let t=__rngState;t=Math.imul(t^(t>>>15),1|t);t=(t+Math.imul(t^(t>>>7),61|t))^t;return((t^(t>>>14))>>>0)/4294967296;}Math.random=__mulberry32;W.Math.random=__mulberry32;
globalThis.fetch=async()=>({ok:true,status:200,text:async()=>'',json:async=>({}),arrayBuffer:async()=>new ArrayBuffer(0),blob:async()=>new W.Blob([],{type:'text/plain'})});
if(W.HTMLImageElement){Object.defineProperty(W.HTMLImageElement.prototype,'src',{configurable:true,set(_v){setTimeout(()=>{this.dispatchEvent&&this.dispatchEvent(new W.Event('error'));},0);},get(){return'';}});Object.defineProperty(W.HTMLImageElement.prototype,'complete',{configurable:true,get(){return true;}});if(typeof W.HTMLImageElement.prototype.decode!=='function')W.HTMLImageElement.prototype.decode=function(){return Promise.resolve();};}
if(W.HTMLCanvasElement){const noop=()=>{};const measureText=()=>({width:0});const makeCtx=()=>({canvas:{width:0,height:0},fillStyle:'',strokeStyle:'',lineWidth:1,font:'',textAlign:'start',textBaseline:'alphabetic',globalAlpha:1,save:noop,restore:noop,translate:noop,rotate:noop,scale:noop,setTransform:noop,beginPath:noop,closePath:noop,moveTo:noop,lineTo:noop,arc:noop,rect:noop,fill:noop,stroke:noop,clip:noop,fillText:noop,strokeText:noop,fillRect:noop,strokeRect:noop,clearRect:noop,measureText,createLinearGradient:()=>({addColorStop:noop}),createRadialGradient:()=>({addColorStop:noop}),drawImage:noop,getImageData:()=>({data:new Uint8ClampedArray(0)}),putImageData:noop});W.HTMLCanvasElement.prototype.getContext=function(){return makeCtx();};}

const mermaid = (await import('mermaid')).default;
mermaid.initialize({ startOnLoad: false, securityLevel: 'loose', handDrawnSeed: 1 });

const src = readFileSync('/ext/mermaid/tests/ext_fixtures/cypress/flowchart/134.mmd', 'utf8');
const { svg } = await mermaid.render('test-134-inner', src);

// Parse the SVG and extract node order from the inner root groups
const parser = new W.DOMParser();
const doc = parser.parseFromString(svg, 'image/svg+xml');

// Find all <g class="root"> elements
const roots = doc.querySelectorAll('g.root');
for (const root of roots) {
  const transform = root.getAttribute('transform');
  // Find the nodes section
  const nodesG = root.querySelector('g.nodes');
  if (nodesG) {
    const nodeIds = [];
    for (const child of nodesG.children) {
      const id = child.getAttribute('id') || '';
      const label = child.textContent?.trim()?.substring(0, 20) || '';
      if (id) nodeIds.push({id, label});
    }
    console.log(`Root (transform=${transform?.substring(0,40)}): nodes=[${nodeIds.map(n=>n.label||n.id.substring(n.id.length-10)).join(', ')}]`);
  }
}

// Also find the outer nodes section
const outerNodes = doc.querySelectorAll('svg > g.nodes');
for (const ng of outerNodes) {
  for (const child of ng.children) {
    const cl = child.getAttribute('class') || '';
    if (cl.includes('root')) {
      // It's a root group, check its transform
      const t = child.getAttribute('transform');
      const innerNodes = child.querySelector('g.nodes');
      if (innerNodes) {
        const labels = [];
        for (const c of innerNodes.children) {
          labels.push(c.textContent?.trim()?.substring(0, 20) || '?');
        }
        console.log(`Root group (${t?.substring(0,30)}): [${labels.join(', ')}]`);
      }
    }
  }
}
