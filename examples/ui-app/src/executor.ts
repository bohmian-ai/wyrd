/**
 * Server-only: reads run as the principal of `WYRD_API_KEY` against
 * `WYRD_SERVER_URL`. The key never leaves this process; pages receive only
 * loaded props.
 */
import { WyrdClient } from 'wyrd-ai';
import { nodeExecutor } from 'wyrd-ai-ui/node';

export const executor = () => nodeExecutor(WyrdClient.connect());
