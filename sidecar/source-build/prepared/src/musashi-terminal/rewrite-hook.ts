import type {IncomingMessage} from 'node:http';
import type {ProxyOptions} from '../config.js';
import type {Session} from '../session.js';
import type {WireProtocol} from '../util.js';
import type {resolveUpstream} from '../server.js';
import {createPromptRewriter,recallPromptRewrites} from './prompt-rewrite.js';
import {createRewriteTransport} from './rewrite-transport.js';
import {bindPromptRewrite,earlierRewriteHumans,selectRewriteHuman,type RewriteNote} from './rewrite-wire.js';
import {CONTROL_MARKER_HEADER,matchControlMarker,type ControlMarker} from './control-marker.js';
const services=new WeakMap<ProxyOptions,ReturnType<typeof createPromptRewriter>>();
const requests=new WeakMap<IncomingMessage,Promise<void>>();
const recognized=new WeakMap<IncomingMessage,ControlMarker|null>();
const mismatchLogged=new WeakSet<Session>();
type Input={req:IncomingMessage;opts:ProxyOptions;route:ReturnType<typeof resolveUpstream>;protocol:WireProtocol;body:Record<string,unknown>;session:Session;model:string;harness:string|undefined;child:boolean;signal:AbortSignal;log:(level:string,message:string)=>void};
/** Shinatama's control messages (hand-off seed, bootstrap, receipt) are declared by a request header carrying the hash of
 * the current human message. Recognized once per request, before anything reads or changes the body: a marked message is
 * never rewritten (no auxiliary call, no catalog fetch), tagged, stored or folded. A header whose hash does not equal the
 * received text is ignored: the request is ordinary. The header may sit on every request of a session, so the mismatch is
 * logged once per session at info and at debug after that. */
export function recognizeControlMarker(req:IncomingMessage,body:Record<string,unknown>,protocol:WireProtocol,session:Session,log:(level:string,message:string)=>void):ControlMarker|undefined{
 const known=recognized.get(req);if(known!==undefined)return known??undefined;
 const header=req.headers[CONTROL_MARKER_HEADER];
 if(header===undefined){recognized.set(req,null);return undefined;}
 const outcome=matchControlMarker(header,selectRewriteHuman(body,protocol));
 recognized.set(req,outcome.marker??null);
 if(outcome.marker)return outcome.marker;
 const first=!mismatchLogged.has(session);mismatchLogged.add(session);
 log(first?'info':'debug','['+session.id+'] control marker mismatch: '+(outcome.kind??'unreadable')+' ('+outcome.reason+'), ordinary treatment');
 return undefined;
}
/** Called only for an admitted main request, before preparing its wire projection. */
export function maybeRewritePrompt(input:Input):Promise<void>{
 const pending=requests.get(input.req);if(pending)return pending;
 const work=apply(input).catch(()=>{input.log('warn','[prompt-rewrite] keeping original after auxiliary failure');});
 requests.set(input.req,work);return work;
}
async function apply(input:Input):Promise<void>{
 const {req,opts,protocol,body,session,harness,signal,log}=input;
 if(recognizeControlMarker(req,body,protocol,session,log))return;
 if(input.child||!harness||!['claude','codex','opencode','pi','gemini'].includes(harness)||opts.promptRewrite?.mode!=='auto'||signal.aborted)return;
 const human=selectRewriteHuman(body,protocol),humans=earlierRewriteHumans(body,protocol,human);
 if(!human&&humans.length===0)return;
 const transport=createRewriteTransport(input);if(!transport)return;
 // Notes already sent after earlier humans come back with the same bytes;
 // only the current human can buy a new rewrite.
 const recalled=recallPromptRewrites({session,harness,scope:transport.scope},humans);
 const earlier:RewriteNote[]=humans.flatMap((previous,index)=>{const prompt=recalled[index];return prompt?[{human:previous,prompt}]:[];});
 let prompt:string|undefined;
 if(human){
  let service=services.get(opts);if(!service){service=createPromptRewriter({diagnostic:event=>log('info','[prompt-rewrite] '+JSON.stringify({outcome:event.outcome,reason:event.reason,model:event.model,selection:event.selection,estimatedCost:event.estimatedCost}))});services.set(opts,service);}
  prompt=await service({session,text:human.text,humanID:human.humanID,harness,scope:transport.scope,currentModel:input.model,settings:()=>opts.promptRewrite,signal,isCurrent:transport.isCurrent,listCatalog:transport.listCatalog,complete:transport.complete});
 }
 if(!signal.aborted&&opts.promptRewrite?.mode==='auto'&&transport.isCurrent())bindPromptRewrite(req,protocol,human,prompt,earlier);
}
