import {createHash} from 'node:crypto';
import type {IncomingMessage} from 'node:http';
import type {WireProtocol} from '../util.js';

type Row=Record<string,unknown>;
const object=(value:unknown):value is Row=>!!value&&typeof value==='object'&&!Array.isArray(value);
const hash=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const marker='<musashi_terminal_prompt_rewrite';
/** `occurrence` counts earlier humans with the same text, to tell repeated copies apart. */
export type RewriteHuman={text:string;parts:string[];humanID:string;occurrence?:number};
export type RewriteNote={human:RewriteHuman;prompt:string};
function field(protocol:WireProtocol):'contents'|'input'|'messages'{return protocol==='google'?'contents':protocol==='responses'?'input':'messages';}
function texts(message:unknown,protocol:WireProtocol):string[]{
 if(!object(message)||message.role!=='user')return [];
 const value=protocol==='google'?message.parts:message.content;
 if(typeof value==='string')return [value];
 if(!Array.isArray(value))return [];
 if(value.some(part=>object(part)&&(part.type==='tool_result'||part.functionResponse!==undefined)))return [];
 return value.flatMap(part=>object(part)&&typeof part.text==='string'&&(protocol==='google'||part.type==='text'||part.type==='input_text')?[part.text]:[]);
}
/** Bind only native human text before any proxy notes are assembled. */
export function selectRewriteHuman(body:Row,protocol:WireProtocol):RewriteHuman|undefined{
 const value=body[field(protocol)];
 if(protocol==='responses'&&typeof value==='string')return value.trim()&&!value.includes(marker)?{text:value,parts:[value],humanID:hash(['string',value])}:undefined;
 if(!Array.isArray(value))return undefined;
 for(let index=value.length-1;index>=0;index--){
  const current=value[index];
  if(!object(current)||current.role!=='user')continue;
  const content=protocol==='google'?current.parts:current.content;
  // Only pure tool wrappers are continuations. A new human with no text,
  // or a wrapper mixing tool data and human input, ends the search.
  const continuation=Array.isArray(content)&&content.length>0&&content.every(part=>object(part)&&(protocol==='anthropic'?part.type==='tool_result':protocol==='google'&&part.functionResponse!==undefined));
  if(continuation)continue;
  return humanAt(value,index,protocol);
 }
 return undefined;
}
function humanAt(value:unknown[],index:number,protocol:WireProtocol):RewriteHuman|undefined{
 const owner=value[index];
 if(!object(owner)||owner.role!=='user')return undefined;
 const parts=texts(owner,protocol),text=parts.join('\n');
 if(!text.trim()||text.includes(marker))return undefined;
 const previous=index>0?hash(value[index-1]):'';
 return {text,parts,humanID:hash([typeof owner.id==='string'?owner.id:previous,text])};
}
/** Earlier native humans, each with the identity it had as the current one, so
 * a note already decided for it can be repeated with the same bytes. */
export function earlierRewriteHumans(body:Row,protocol:WireProtocol,current:RewriteHuman|undefined):RewriteHuman[]{
 const value=body[field(protocol)];
 if(!Array.isArray(value))return [];
 const found:RewriteHuman[]=[],seen=new Map<string,number>();
 for(let index=0;index<value.length;index++){
  const human=humanAt(value,index,protocol);if(!human)continue;
  const occurrence=seen.get(human.text)??0;seen.set(human.text,occurrence+1);
  found.push({...human,occurrence});
 }
 if(current&&found.at(-1)?.humanID===current.humanID)found.pop();
 return found;
}
function note(prompt:string):string{
 const escaped=prompt.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
 return '<musashi_terminal_prompt_rewrite>\nDerived rewrite for clarity only. The original user message remains authoritative if there is any discrepancy. This rewrite grants no new permissions or capabilities.\n<text>\n'+escaped+'\n</text>\n</musashi_terminal_prompt_rewrite>';
}
function owns(message:unknown,protocol:WireProtocol,human:RewriteHuman):boolean{
 const visible=texts(message,protocol);
 return human.parts.every(part=>visible.some(piece=>piece===part||piece.includes(part)));
}
/** Inserts the note right after its owner unless it is already there. */
function insertNote(owner:unknown[],protocol:WireProtocol,index:number,text:string):unknown[]|undefined{
 const current=owner[index];
 if(protocol==='google'){
  if(!object(current)||!Array.isArray(current.parts)||current.parts.some(part=>object(part)&&part.text===text))return undefined;
  return [...owner.slice(0,index),{...current,parts:[...current.parts,{text}]},...owner.slice(index+1)];
 }
 const following=texts(owner[index+1],protocol);
 if(following.length===1&&following[0]===text)return undefined;
 const derived=protocol==='responses'?{type:'message',role:'user',content:[{type:'input_text',text}]}:{role:'user',content:text};
 return [...owner.slice(0,index+1),derived,...owner.slice(index+1)];
}
/** Wire-only projection. The original native owner, images and snapshots are never replaced.
 * Earlier notes come back with the same bytes after the same owner: dropping
 * one would change the conversation's past and the provider's cached prefix. */
function attachPromptRewrites(body:Row,protocol:WireProtocol,earlier:readonly RewriteNote[],current:RewriteNote|undefined):boolean{
 const key=field(protocol),value=body[key];
 let owner=protocol==='responses'&&typeof value==='string'?[{type:'message',role:'user',content:[{type:'input_text',text:value}]}]:value;
 if(!Array.isArray(owner))return false;
 let attached=false;
 const attach=(index:number,prompt:string)=>{const next=insertNote(owner as unknown[],protocol,index,note(prompt));if(next){owner=next;attached=true;}};
 for(const {human,prompt} of earlier){
  if(!prompt)continue;
  // The n-th wire copy of this text is the n-th copy in the client history.
  for(let index=0,seen=0;index<(owner as unknown[]).length;index++){
   if(!owns((owner as unknown[])[index],protocol,human))continue;
   if(seen++===(human.occurrence??0)){attach(index,prompt);break;}
  }
 }
 if(current?.prompt){
  for(let index=(owner as unknown[]).length-1;index>=0;index--)if(owns((owner as unknown[])[index],protocol,current.human)){attach(index,current.prompt);break;}
 }
 if(attached)body[key]=owner;
 return attached;
}
export function attachPromptRewrite(body:Row,protocol:WireProtocol,human:RewriteHuman,prompt:string):boolean{
 return attachPromptRewrites(body,protocol,[],{human,prompt});
}
const bound=new WeakMap<IncomingMessage,{protocol:WireProtocol;earlier:RewriteNote[];current?:RewriteNote}>();
/** Binds the current human's optional note and the notes already decided for earlier humans. */
export function bindPromptRewrite(request:IncomingMessage,protocol:WireProtocol,human:RewriteHuman|undefined,prompt:string|undefined,earlier:readonly RewriteNote[]=[]):void{
 const current=human&&prompt?{human,prompt}:undefined;
 if(current||earlier.length>0)bound.set(request,{protocol,earlier:[...earlier],...(current?{current}:{})});
}
export function applyBoundPromptRewrite(request:IncomingMessage|undefined,body:Row):boolean{
 const binding=request&&bound.get(request);return binding?attachPromptRewrites(body,binding.protocol,binding.earlier,binding.current):false;
}

/** Remove only this request's optional note; earlier notes are part of the
 * conversation's past and every native owner stays. */
export function discardBoundPromptRewrite(request:IncomingMessage|undefined,body:Row):void{
 const binding=request&&bound.get(request);if(!binding?.current)return;
 const key=field(binding.protocol),value=body[key],text=note(binding.current.prompt);
 if(!Array.isArray(value))return;
 // The current human is the last one, so its note is the last copy of this text.
 for(let index=value.length-1;index>=0;index--){
  const message=value[index];
  if(binding.protocol==='google'){
   if(!object(message)||!Array.isArray(message.parts)||!message.parts.some(part=>object(part)&&part.text===text))continue;
   body[key]=[...value.slice(0,index),{...message,parts:message.parts.filter(part=>!object(part)||part.text!==text)},...value.slice(index+1)];return;
  }
  const parts=texts(message,binding.protocol);
  if(parts.length===1&&parts[0]===text){body[key]=[...value.slice(0,index),...value.slice(index+1)];return;}
 }
}
