// Zero-dependency runtime lock that normalizes process.env.USER_TYPE for the
// public Mossen build. Internal user types ('ant', 'mossen') only pass through
// when the explicit unlock env var is set; everything else collapses to
// 'external'. This prevents accidental activation of internal-only paths via
// `export USER_TYPE=ant` while preserving an explicit escape hatch for
// internal/enterprise/controlled-test deployments.
//
// Must remain zero-import: callable from any layer including pre-bootstrap
// entrypoint code without dragging in sessionStorage / config / auth / tools.

const PUBLIC_USER_TYPE = 'external'

export function isInternalUserTypeUnlocked(): boolean {
  return process.env.MOSSEN_CODE_ALLOW_INTERNAL_USER_TYPE === '1'
}

export function normalizeUserType(
  raw: string | undefined = process.env.USER_TYPE,
): string {
  if (raw === PUBLIC_USER_TYPE) return PUBLIC_USER_TYPE
  if (!raw) return PUBLIC_USER_TYPE
  if ((raw === 'ant' || raw === 'mossen') && isInternalUserTypeUnlocked()) {
    return raw
  }
  return PUBLIC_USER_TYPE
}

export function applyUserTypeRuntimeLock(): void {
  process.env.USER_TYPE = normalizeUserType(process.env.USER_TYPE)
}
