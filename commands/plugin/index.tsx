import type { Command } from '../../commands.js';
import { getProductDisplayName } from '../../constants/product.js';
const plugin = {
  type: 'local-jsx',
  name: 'plugin',
  aliases: ['plugins', 'marketplace'],
  description: `Manage ${getProductDisplayName()} plugins`,
  immediate: true,
  load: () => import('./plugin.js')
} satisfies Command;
export default plugin;
