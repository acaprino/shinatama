import type {IncomingMessage} from 'node:http';
import type {ProxyOptions} from '../config.js';
import type {Session} from '../session.js';
import type {WireProtocol} from '../util.js';
import type {resolveUpstream} from '../server.js';
import {createPromptRewriter,recallPromptRewrites} from './prompt-rewrite.js';
import {createRewriteTransport} from './rewrite-transport.js';
import {bindPromptRewrite,earlierRewriteHumans,selectRewriteHuman,type RewriteNote} from './rewrite-wire.js';
const services=new WeakMap<ProxyOptions,ReturnType<typeof createPromptRewriter>>();
const requests=new WeakMap<IncomingMessage,Promise<void>>();
type Input={req:IncomingMessage;opts:ProxyOptions;route:ReturnType<typeof resolveUpstream>;protocol:WireProtocol;body:Record<string,unknown>;session:Session;model:string;harness:string|undefined;child:boolean;signal:AbortSignal;log:(level:string,message:string)=>void};
/** Called only for an admitted main request, before preparing its wire projection. */
export function maybeRewritePrompt(input:Input):Promise<void>{
 const pending=requests.get(input.req);if(pending)return pending;
 const work=apply(input).catch(()=>{input.log('warn','[prompt-rewrite] keeping original after auxiliary failure');});
 requests.set(input.req,work);return work;
}
async function apply(input:Input):Promise<void>{
 const {req,opts,protocol,body,session,harness,signal,log}=input;
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
