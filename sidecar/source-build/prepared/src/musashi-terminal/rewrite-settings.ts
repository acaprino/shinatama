import type {PromptRewriteSettings} from './prompt-rewrite.js';
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
/** Absent/malformed configuration cannot silently start an auxiliary purchase. */
export function parsePromptRewriteSettings(value:unknown):PromptRewriteSettings|undefined{
 if(!object(value)||!Object.hasOwn(value,'mode')||!['auto','off'].includes(String(value.mode)))return undefined;
 const maxCostUsd=Object.hasOwn(value,'maxCostUsd')?value.maxCostUsd:0.01;
 if(typeof maxCostUsd!=='number'||!Number.isFinite(maxCostUsd)||maxCostUsd<=0||maxCostUsd>1)return undefined;
 const raw=Object.hasOwn(value,'preferredModels')?value.preferredModels:{};
 if(!object(raw))return undefined;
 const preferredModels:Record<string,string[]>={};
 for(const [scope,models] of Object.entries(raw)){
  if(['__proto__','prototype','constructor'].includes(scope))continue;
  if(!scope||scope.length>4096||!Array.isArray(models)||models.length>32||models.some(model=>typeof model!=='string'||!model||model.length>512||/[\s\x00-\x1f]/.test(model)))return undefined;
  preferredModels[scope]=[...new Set(models as string[])];
 }
 return {mode:value.mode as 'auto'|'off',maxCostUsd,preferredModels};
}
