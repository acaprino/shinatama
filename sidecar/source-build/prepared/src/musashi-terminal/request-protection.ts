import type { Config, CoreMessage } from 'acp-kernel';
import type { ResolvedKernelConfig } from '../compress-settings.js';

/** Musashi Terminal: the current human request and system instructions never become fold candidates. */
export function protectAdaptiveMessages(config: Config, messages: CoreMessage[]): Config {
    if (!(config as ResolvedKernelConfig).adaptivePressure?.enabled) return config;
    const current = messages.findLast(message => message.role === 'user' && message.contentType === 'text');
    const ownerIndex = (message: CoreMessage | undefined): number | undefined => {
        const index = (message as (CoreMessage & { wireIndex?: number }) | undefined)?.wireIndex;
        return Number.isSafeInteger(index) && index! >= 0 ? index : undefined;
    };
    const currentOwner = ownerIndex(current);
    const previous = config.isMessageProtected;
    return { ...config, isMessageProtected: message => message.role === 'system' || message.id === current?.id
        || currentOwner !== undefined && ownerIndex(message) === currentOwner || previous?.(message) === true };
}
