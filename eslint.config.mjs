// Mossen 最小 ESLint 配置。
// 目的：架起闸门（bun run lint），挡未来新增明显错误；历史遗留先不强制清零。
//
// 配套 P0-05 typecheck:diff baseline gate，两层网：tsc 抓类型/import，eslint 抓
// react hooks 规则、未用变量、@ts-ignore 滥用等。
//
// 设计原则：
// - strict false（刚落地，不要一上来挡住所有提交）
// - no-unused-vars / no-undef 交给 ts（避免双报）
// - react-hooks/rules-of-hooks 必错（这是真 bug 类别）
// - no-console warn
// - 忽略生成文件、.mossen/、.mossensrc/、node_modules/、types/generated/
//
// Slice A/4 of P0-06.

import tsParser from '@typescript-eslint/parser'
import tsPlugin from '@typescript-eslint/eslint-plugin'
import reactPlugin from 'eslint-plugin-react'
import reactHooksPlugin from 'eslint-plugin-react-hooks'

// 上游 Claude Code 仓库带有 custom-rules/* eslint-disable 注释（约 ~16 条规则）
// 但 plugin 实现没随源码迁移过来。提供一个空实现 stub，让 ESLint 不再报
// "Definition for rule ... was not found"。这些 disable 反正本来也没起作用。
// 后续 P0-06-D 可考虑批量删 disable 注释。
function makeStubPlugin(ruleNames) {
  const rules = {}
  for (const name of ruleNames) {
    rules[name] = {
      meta: { type: 'problem', schema: [] },
      create() { return {} },
    }
  }
  return { rules }
}

// 上游 Claude Code 仓库带有 custom-rules/* eslint-disable 注释（约 ~16 条规则）
// 但 plugin 实现没随源码迁移过来。提供空实现 stub 让 ESLint 不再报
// "Definition for rule ... was not found"。这些 disable 反正本来也没起作用。
// 后续 P0-06-D 可考虑批量删 disable 注释。
const CUSTOM_RULES_STUB = makeStubPlugin([
  'bootstrap-isolation',
  'no-cross-platform-process-issues',
  'no-direct-json-operations',
  'no-direct-ps-commands',
  'no-lookbehind-regex',
  'no-process-cwd',
  'no-process-env-top-level',
  'no-process-exit',
  'no-sync-fs',
  'no-top-level-dynamic-import',
  'no-top-level-side-effects',
  'prefer-use-keybindings',
  'prefer-use-terminal-size',
  'prompt-spacing',
  'require-bun-typeof-guard',
  'require-tool-match-name',
])

// eslint-plugin-n 也是上游遗产；同样 stub
const N_PLUGIN_STUB = makeStubPlugin([
  'no-unsupported-features/node-builtins',
  'no-sync',
])

/** @type {import('eslint').Linter.FlatConfig[]} */
export default [
  {
    ignores: [
      'node_modules/**',
      'dist/**',
      '.mossen/**',
      '.mossensrc/**',
      '.git/**',
      'tmp/**',
      'coverage/**',
      'outputs/**',
      'types/generated/**',
      '**/*.bak-*',
    ],
  },
  {
    files: ['**/*.ts', '**/*.tsx'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
        ecmaFeatures: { jsx: true },
      },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
      react: reactPlugin,
      'react-hooks': reactHooksPlugin,
      'custom-rules': CUSTOM_RULES_STUB,
      'eslint-plugin-n': N_PLUGIN_STUB,
    },
    rules: {
      // ts 自己管更准，eslint 版容易和 bundler 不一致
      'no-unused-vars': 'off',
      'no-undef': 'off',

      // ts-eslint 版 no-unused-vars：以 _ 前缀作为"故意未用"的约定
      '@typescript-eslint/no-unused-vars': [
        'warn',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],

      // React hooks 规则是真 bug 类别（顺序错会 runtime 崩）
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',

      // 非错误路径 console.log 应清到 logForDebugging / logError
      'no-console': ['warn', { allow: ['warn', 'error'] }],

      // 历史债：仓库有真 as any；新代码尽量避免
      '@typescript-eslint/no-explicit-any': 'warn',

      // 强制 @ts-expect-error 有说明
      '@typescript-eslint/ban-ts-comment': [
        'warn',
        {
          'ts-ignore': true,
          'ts-nocheck': true,
          'ts-expect-error': 'allow-with-description',
          minimumDescriptionLength: 4,
        },
      ],
    },
    settings: {
      react: { version: 'detect' },
    },
  },
]
