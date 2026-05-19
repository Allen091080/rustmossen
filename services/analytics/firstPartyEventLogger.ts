import { randomUUID } from 'crypto'
import { isEqual } from 'lodash-es'
import { getSessionId } from '../../bootstrap/state.js'
import { GrowthbookExperimentEvent } from '../../types/generated/events_mono/growthbook/v1/growthbook_experiment_event.js'
import { MossenCodeInternalEvent } from '../../types/generated/events_mono/mossen_code/v1/mossen_code_internal_event.js'
import { getOrCreateUserID } from '../../utils/config.js'
import { logForDebugging } from '../../utils/debug.js'
import { logError } from '../../utils/log.js'
import { getUserType } from '../../utils/userType.js'
import { jsonStringify } from '../../utils/slowOperations.js'
import { profileCheckpoint } from '../../utils/startupProfiler.js'
import { getCoreUserData } from '../../utils/user.js'
import { isAnalyticsDisabled } from './config.js'
import { FirstPartyEventExporter } from './firstPartyEventExporter.js'
import type { GrowthBookUserAttributes } from './growthbook.js'
import { getDynamicConfig_CACHED_MAY_BE_STALE } from './growthbook.js'
import { stripProtoFields } from './index.js'
import { getEventMetadata, to1PEventFormat } from './metadata.js'
import { isSinkKilled } from './sinkKillswitch.js'

/**
 * Configuration for sampling individual event types.
 * Each event name maps to an object containing sample_rate (0-1).
 * Events not in the config are logged at 100% rate.
 */
export type EventSamplingConfig = {
  [eventName: string]: {
    sample_rate: number
  }
}

const EVENT_SAMPLING_CONFIG_NAME = 'tengu_event_sampling_config'
/**
 * Get the event sampling configuration from GrowthBook.
 * Uses cached value if available, updates cache in background.
 */
export function getEventSamplingConfig(): EventSamplingConfig {
  return getDynamicConfig_CACHED_MAY_BE_STALE<EventSamplingConfig>(
    EVENT_SAMPLING_CONFIG_NAME,
    {},
  )
}

/**
 * Determine if an event should be sampled based on its sample rate.
 * Returns the sample rate if sampled, null if not sampled.
 *
 * @param eventName - Name of the event to check
 * @returns The sample_rate if event should be logged, null if it should be dropped
 */
export function shouldSampleEvent(eventName: string): number | null {
  const config = getEventSamplingConfig()
  const eventConfig = config[eventName]

  // If no config for this event, log at 100% rate (no sampling)
  if (!eventConfig) {
    return null
  }

  const sampleRate = eventConfig.sample_rate

  // Validate sample rate is in valid range
  if (typeof sampleRate !== 'number' || sampleRate < 0 || sampleRate > 1) {
    return null
  }

  // Sample rate of 1 means log everything (no need to add metadata)
  if (sampleRate >= 1) {
    return null
  }

  // Sample rate of 0 means drop everything
  if (sampleRate <= 0) {
    return 0
  }

  // Randomly decide whether to sample this event
  return Math.random() < sampleRate ? sampleRate : 0
}

const BATCH_CONFIG_NAME = 'tengu_1p_event_batch_config'
type BatchConfig = {
  scheduledDelayMillis?: number
  maxExportBatchSize?: number
  maxQueueSize?: number
  skipAuth?: boolean
  maxAttempts?: number
  path?: string
  baseUrl?: string
}
function getBatchConfig(): BatchConfig {
  return getDynamicConfig_CACHED_MAY_BE_STALE<BatchConfig>(
    BATCH_CONFIG_NAME,
    {},
  )
}

// Module-local state for event logging (not exposed globally).
// Y-5: removed firstPartyEventLogger / firstPartyEventLoggerProvider
// (zero-OTel pipeline). _newEventExporter is now the only path.
let _newEventExporter: FirstPartyEventExporter | null = null
// Last batch config used to construct the exporter — used by
// reinitialize1PEventLoggingIfConfigChanged to decide whether a rebuild is
// needed when GrowthBook refreshes.
let lastBatchConfig: BatchConfig | null = null
/**
 * Flush and shutdown the 1P event logger.
 * This should be called as the final step before process exit to ensure
 * all events (including late ones from API responses) are exported.
 */
export async function shutdown1PEventLogging(): Promise<void> {
  if (!_newEventExporter) return
  try {
    await _newEventExporter.shutdown()
    if (getUserType() === 'ant') {
      logForDebugging('1P event logging: final shutdown complete')
    }
  } catch {
    // ignore shutdown errors
  }
  _newEventExporter = null
}

/**
 * Check if 1P event logging is enabled.
 * Respects the same opt-outs as other analytics sinks:
 * - Test environment
 * - Third-party cloud providers (Bedrock/Vertex)
 * - Global telemetry opt-outs
 * - Non-essential traffic disabled
 *
 * Note: Unlike BigQuery metrics, event logging does NOT check organization-level
 * metrics opt-out via API. It follows the same pattern as Statsig event logging.
 */
export function is1PEventLoggingEnabled(): boolean {
  // Respect standard analytics opt-outs
  return !isAnalyticsDisabled()
}

/**
 * Log a 1st-party event for internal analytics (async version).
 * Events are batched and exported to /api/event_logging/batch
 *
 * This enriches the event with core metadata (model, session, env context, etc.)
 * at log time, similar to logEventToStatsig.
 *
 * Y-4: routes to FirstPartyEventExporter (zero-OTel). Transform from raw
 * metadata → MossenCodeInternalEvent proto is inlined here (was previously
 * deferred until OTel BatchLogRecordProcessor's transformLogsToEvents).
 *
 * @param eventName - Name of the event (e.g., 'tengu_api_query')
 * @param metadata - Additional metadata for the event (intentionally no strings, to avoid accidentally logging code/filepaths)
 */
async function logEventTo1PAsync(
  exporter: FirstPartyEventExporter,
  eventName: string,
  metadata: Record<string, number | boolean | undefined> = {},
): Promise<void> {
  try {
    const coreMetadata = await getEventMetadata({
      model: metadata.model,
      betas: metadata.betas,
    })
    const userMetadata = getCoreUserData(true)
    const userId = getOrCreateUserID()
    const eventId = randomUUID()
    const clientTimestamp = new Date()

    if (getUserType() === 'ant') {
      logForDebugging(
        `[MOSSEN-INTERNAL] 1P event: ${eventName} ${jsonStringify(metadata, null, 0)}`,
      )
    }

    if (!coreMetadata) {
      // Partial event when core metadata enrichment failed.
      if (getUserType() === 'ant') {
        logForDebugging(
          `1P event logging: core_metadata missing for event ${eventName}`,
        )
      }
      exporter.enqueue({
        event_type: 'MossenCodeInternalEvent',
        event_data: MossenCodeInternalEvent.toJSON({
          event_id: eventId,
          event_name: eventName,
          client_timestamp: clientTimestamp,
          session_id: getSessionId(),
          additional_metadata: Buffer.from(
            jsonStringify({
              transform_error: 'core_metadata attribute is missing',
            }),
          ).toString('base64'),
        }),
      })
      return
    }

    const formatted = to1PEventFormat(coreMetadata, userMetadata, metadata)
    // _PROTO_* keys are PII-tagged values meant only for privileged BQ
    // columns. Hoist known keys to proto fields, then defensively strip any
    // remaining _PROTO_* so an unrecognized future key can't silently land
    // in the general-access additional_metadata blob.
    const {
      _PROTO_skill_name,
      _PROTO_plugin_name,
      _PROTO_marketplace_name,
      ...rest
    } = formatted.additional
    const additionalMetadata = stripProtoFields(rest)

    exporter.enqueue({
      event_type: 'MossenCodeInternalEvent',
      event_data: MossenCodeInternalEvent.toJSON({
        event_id: eventId,
        event_name: eventName,
        client_timestamp: clientTimestamp,
        device_id: userId,
        email: userMetadata?.email,
        auth: formatted.auth,
        ...formatted.core,
        env: formatted.env,
        process: formatted.process,
        skill_name:
          typeof _PROTO_skill_name === 'string'
            ? _PROTO_skill_name
            : undefined,
        plugin_name:
          typeof _PROTO_plugin_name === 'string'
            ? _PROTO_plugin_name
            : undefined,
        marketplace_name:
          typeof _PROTO_marketplace_name === 'string'
            ? _PROTO_marketplace_name
            : undefined,
        additional_metadata:
          Object.keys(additionalMetadata).length > 0
            ? Buffer.from(jsonStringify(additionalMetadata)).toString('base64')
            : undefined,
      }),
    })
  } catch (e) {
    if (process.env.NODE_ENV === 'development') {
      throw e
    }
    if (getUserType() === 'ant') {
      logError(e as Error)
    }
    // swallow
  }
}

/**
 * Log a 1st-party event for internal analytics.
 * Events are batched and exported to /api/event_logging/batch
 *
 * @param eventName - Name of the event (e.g., 'tengu_api_query')
 * @param metadata - Additional metadata for the event (intentionally no strings, to avoid accidentally logging code/filepaths)
 */
export function logEventTo1P(
  eventName: string,
  metadata: Record<string, number | boolean | undefined> = {},
): void {
  if (!is1PEventLoggingEnabled()) {
    return
  }

  // Y-4: route to new (zero-OTel) exporter. The OTel logger and provider
  // are still constructed by initialize1PEventLogging() but no longer
  // receive enqueues — they become a dead branch awaiting Y-5 deletion.
  if (!_newEventExporter || isSinkKilled('firstParty')) {
    return
  }

  // Fire and forget - don't block on metadata enrichment
  void logEventTo1PAsync(_newEventExporter, eventName, metadata)
}

/**
 * GrowthBook experiment event data for logging
 */
export type GrowthBookExperimentData = {
  experimentId: string
  variationId: number
  userAttributes?: GrowthBookUserAttributes
  experimentMetadata?: Record<string, unknown>
}

// Mossen platform exports the production GrowthBook environment to this client.
function getEnvironmentForGrowthBook(): string {
  return 'production'
}

/**
 * Log a GrowthBook experiment assignment event to 1P.
 * Events are batched and exported to /api/event_logging/batch
 *
 * @param data - GrowthBook experiment assignment data
 */
export function logGrowthBookExperimentTo1P(
  data: GrowthBookExperimentData,
): void {
  if (!is1PEventLoggingEnabled()) {
    return
  }

  // Y-4: route to new (zero-OTel) exporter, same gate semantics as
  // logEventTo1P (sink killswitch + exporter present).
  if (!_newEventExporter || isSinkKilled('firstParty')) {
    return
  }

  const userId = getOrCreateUserID()
  const { accountUuid, organizationUuid } = getCoreUserData(true)
  const eventId = randomUUID()
  const timestamp = new Date()

  if (getUserType() === 'ant') {
    logForDebugging(
      `[MOSSEN-INTERNAL] 1P GrowthBook experiment: ${data.experimentId} variation=${data.variationId}`,
    )
  }

  _newEventExporter.enqueue({
    event_type: 'GrowthbookExperimentEvent',
    event_data: GrowthbookExperimentEvent.toJSON({
      event_id: eventId,
      timestamp,
      experiment_id: data.experimentId,
      variation_id: data.variationId,
      environment: getEnvironmentForGrowthBook(),
      user_attributes: data.userAttributes
        ? jsonStringify(data.userAttributes)
        : undefined,
      experiment_metadata: data.experimentMetadata
        ? jsonStringify(data.experimentMetadata)
        : undefined,
      device_id: userId,
      session_id: data.userAttributes?.sessionId,
      auth:
        accountUuid || organizationUuid
          ? { account_uuid: accountUuid, organization_uuid: organizationUuid }
          : undefined,
    }),
  })
}

const DEFAULT_LOGS_EXPORT_INTERVAL_MS = 10000
const DEFAULT_MAX_EXPORT_BATCH_SIZE = 200
const DEFAULT_MAX_QUEUE_SIZE = 8192

/**
 * Initialize 1P event logging infrastructure.
 * This creates a separate LoggerProvider for internal event logging,
 * independent of customer OTLP telemetry.
 *
 * This uses its own minimal resource configuration with just the attributes
 * we need for internal analytics (service name, version, platform info).
 */
export function initialize1PEventLogging(): void {
  profileCheckpoint('1p_event_logging_start')
  const enabled = is1PEventLoggingEnabled()

  if (!enabled) {
    if (getUserType() === 'ant') {
      logForDebugging('1P event logging not enabled')
    }
    return
  }

  // Fetch batch processor configuration from GrowthBook dynamic config
  // Uses cached value if available, refreshes in background
  const batchConfig = getBatchConfig()
  lastBatchConfig = batchConfig
  profileCheckpoint('1p_event_after_growthbook_config')

  const scheduledDelayMillis =
    batchConfig.scheduledDelayMillis ||
    parseInt(
      process.env.OTEL_LOGS_EXPORT_INTERVAL ||
        DEFAULT_LOGS_EXPORT_INTERVAL_MS.toString(),
    )

  const maxExportBatchSize =
    batchConfig.maxExportBatchSize || DEFAULT_MAX_EXPORT_BATCH_SIZE

  const maxQueueSize = batchConfig.maxQueueSize || DEFAULT_MAX_QUEUE_SIZE

  // Y-5: 1P event pipeline is now zero-OTel. Construct only the new
  // FirstPartyEventExporter (Y-2). Killswitch + isAnalyticsDisabled() gate
  // the network the same way as the prior OTel pipeline.
  _newEventExporter = new FirstPartyEventExporter({
    maxBatchSize: maxExportBatchSize,
    maxQueueSize,
    scheduledDelayMillis,
    skipAuth: batchConfig.skipAuth,
    maxAttempts: batchConfig.maxAttempts,
    path: batchConfig.path,
    baseUrl: batchConfig.baseUrl,
    isKilled: () => isSinkKilled('firstParty'),
  })
}

/**
 * Rebuild the 1P event logging pipeline if the batch config changed.
 * Register this with onGrowthBookRefresh so long-running sessions pick up
 * changes to batch size, delay, endpoint, etc.
 *
 * Event-loss safety:
 * 1. Null the exporter first — concurrent logEventTo1P() calls hit the
 *    !_newEventExporter guard and bail during the swap window. This drops
 *    a handful of events but prevents emitting to a draining exporter.
 * 2. forceFlush() drains the old EventQueue buffer to the network/disk.
 *    Export failures go to disk at <storageDir>/1p_failed_events.<sid>.<uuid>
 *    keyed by module-level BATCH_UUID + sessionId — the NEW exporter's
 *    retryPreviousBatches() picks them up at construction time.
 * 3. Swap to new exporter; old exporter shutdown runs in background.
 */
export async function reinitialize1PEventLoggingIfConfigChanged(): Promise<void> {
  if (!is1PEventLoggingEnabled() || !_newEventExporter) {
    return
  }

  const newConfig = getBatchConfig()

  if (isEqual(newConfig, lastBatchConfig)) {
    return
  }

  if (getUserType() === 'ant') {
    logForDebugging(
      `1P event logging: ${BATCH_CONFIG_NAME} changed, reinitializing`,
    )
  }

  const oldExporter = _newEventExporter
  _newEventExporter = null

  try {
    await oldExporter.forceFlush()
  } catch {
    // Export failures are already on disk; new exporter will retry them.
  }

  try {
    initialize1PEventLogging()
  } catch (e) {
    // Restore so the next GrowthBook refresh can retry. oldExporter was
    // only forceFlush()'d, not shut down — it's still functional. Without
    // this, _newEventExporter stays null and the !_newEventExporter gate at
    // the top makes recovery impossible.
    _newEventExporter = oldExporter
    logError(e)
    return
  }

  void oldExporter.shutdown().catch(() => {})
}
