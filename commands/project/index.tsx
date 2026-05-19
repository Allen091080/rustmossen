import type { Command } from '../../commands.js';

const project = {
  type: 'local-jsx',
  name: 'project',
  aliases: [],
  description: 'Manage project storage (purge sessions, preserve memory)',
  immediate: true,
  load: () => import('./project.js'),
} satisfies Command;

export default project;
