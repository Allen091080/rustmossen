import figures from 'figures';
import * as React from 'react';
import { useEffect } from 'react';
import { Box, Text } from '../../ink.js';
import { errorMessage } from '../../utils/errors.js';
import { logError } from '../../utils/log.js';
import {
  describePluginStatus,
  type PluginStatusSummary,
} from '../../utils/plugins/statusOps.js';
import { plural } from '../../utils/stringUtils.js';
import { getLocalizedText } from '../../utils/uiLanguage.js';

type Props = {
  onComplete: (result?: string) => void;
};

const SIZE_UNITS = ['B', 'KB', 'MB', 'GB'] as const;

function formatBytes(n: number): string {
  if (n < 0) return getLocalizedText({ en: '(unknown)', zh: '（未知）' });
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < SIZE_UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(value < 10 && unit > 0 ? 1 : 0)}${SIZE_UNITS[unit]}`;
}

function formatStatus(s: PluginStatusSummary): string {
  const lines: string[] = [];
  lines.push(
    getLocalizedText({
      en: `${figures.info} Plugin status (read-only)`,
      zh: `${figures.info} 插件状态（只读）`,
    }),
  );
  lines.push('');

  lines.push(
    getLocalizedText({
      en: `Plugin root:    ${s.pluginRootPath}  ${s.pluginRootExists ? '(exists)' : '(absent)'}`,
      zh: `插件根目录:     ${s.pluginRootPath}  ${s.pluginRootExists ? '（存在）' : '（不存在）'}`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `Cache path:     ${s.cache.cachePath}`,
      zh: `Cache 路径:     ${s.cache.cachePath}`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `Marketplaces:   ${s.marketplacesDir}  ${s.marketplacesDirExists ? '(exists)' : '(absent)'}`,
      zh: `Marketplaces:   ${s.marketplacesDir}  ${s.marketplacesDirExists ? '（存在）' : '（不存在）'}`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `Installed reg:  ${s.installedRegistryPath}  ${s.installedRegistryLoadable ? '(loadable)' : '(load failed)'}`,
      zh: `installed 注册表: ${s.installedRegistryPath}  ${s.installedRegistryLoadable ? '（可加载）' : '（加载失败）'}`,
    }),
  );
  lines.push('');

  if (s.cache.zipCacheMode) {
    lines.push(
      getLocalizedText({
        en:
          'Zip-cache mode is active — /plugin prune does not operate on zip caches.\n' +
          'No orphan walk performed.',
        zh:
          '当前启用 zip 缓存模式 —— /plugin prune 不处理 zip 缓存。\n' +
          '未执行 orphan 扫描。',
      }),
    );
    lines.push('');
    lines.push(
      getLocalizedText({
        en: `Suggested: ${s.suggestedCommand}`,
        zh: `建议: ${s.suggestedCommand}`,
      }),
    );
    return lines.join('\n');
  }

  lines.push(
    getLocalizedText({
      en: 'Registry counts:',
      zh: 'Registry 统计:',
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  installed plugins:  ${s.installedRecordCount} ${plural(s.installedRecordCount, 'record')}`,
      zh: `  已安装插件:         ${s.installedRecordCount} 条`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  installed versions: ${s.installedVersionCount}`,
      zh: `  已安装版本:         ${s.installedVersionCount} 个`,
    }),
  );
  lines.push('');

  lines.push(
    getLocalizedText({
      en: 'Cache counts:',
      zh: 'Cache 统计:',
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  marketplaces:       ${s.cache.marketplaceCount}`,
      zh: `  marketplace 数:     ${s.cache.marketplaceCount}`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  unique plugins:     ${s.cache.uniquePluginCount}`,
      zh: `  唯一插件数:         ${s.cache.uniquePluginCount}`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  cache versions:     ${s.cache.cacheVersionCount}`,
      zh: `  cache 版本数:       ${s.cache.cacheVersionCount}`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  cache total bytes:  ${formatBytes(s.cache.cacheBytes)}`,
      zh: `  cache 总大小:       ${formatBytes(s.cache.cacheBytes)}`,
    }),
  );
  lines.push('');

  lines.push(
    getLocalizedText({
      en: 'Orphan classification (W55 R1 idiom):',
      zh: 'Orphan 分类（W55 R1 idiom）:',
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  expired (>7d):      ${s.cache.expiredOrphanCount}  — would be deleted on /plugin prune --confirm`,
      zh: `  过期（>7 天）:      ${s.cache.expiredOrphanCount}  — /plugin prune --confirm 后将删除`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  unmarked:           ${s.cache.unmarkedOrphanCount}  — would be marked on /plugin prune --confirm`,
      zh: `  未标记:             ${s.cache.unmarkedOrphanCount}  — /plugin prune --confirm 后仅标记`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  fresh (<=7d):       ${s.cache.freshOrphanCount}  — held by 7-day grace`,
      zh: `  新鲜（<=7 天）:     ${s.cache.freshOrphanCount}  — 7 天宽限期内保留`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `  installed-skipped:  ${s.cache.installedSkippedCount}  — protected (in installed registry)`,
      zh: `  已安装跳过:         ${s.cache.installedSkippedCount}  — 受保护（在 installed 注册表中）`,
    }),
  );
  lines.push('');

  lines.push(
    getLocalizedText({
      en: `Prune eligibility: ${s.pruneEligible ? 'YES — orphans present' : 'no orphans'}`,
      zh: `Prune 资格: ${s.pruneEligible ? '是 —— 存在 orphan' : '无 orphan'}`,
    }),
  );
  lines.push(
    getLocalizedText({
      en: `Suggested:         ${s.suggestedCommand}`,
      zh: `建议:               ${s.suggestedCommand}`,
    }),
  );
  return lines.join('\n');
}

export function PluginStatus({ onComplete }: Props): React.ReactNode {
  useEffect(() => {
    let cancelled = false;
    const run = async (): Promise<void> => {
      try {
        const status = await describePluginStatus();
        if (cancelled) return;
        onComplete(formatStatus(status));
      } catch (error) {
        if (cancelled) return;
        logError(error);
        onComplete(
          getLocalizedText({
            en: `${figures.cross} /plugin status failed: ${errorMessage(error)}`,
            zh: `${figures.cross} /plugin status 失败: ${errorMessage(error)}`,
          }),
        );
      }
    };
    run();
    return () => {
      cancelled = true;
    };
  }, [onComplete]);

  return (
    <Box>
      <Text dimColor>
        {getLocalizedText({
          en: 'Computing plugin status…',
          zh: '正在计算插件状态…',
        })}
      </Text>
    </Box>
  );
}
