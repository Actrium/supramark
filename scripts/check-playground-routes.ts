#!/usr/bin/env node
/**
 * Consistency check: the playground's route table, its feature registry, and
 * the feature packages the preview CLI can discover must all agree.
 *
 * Background: `bun run feature:preview:web` lists features by scanning
 * `packages/` for `@supramark/feature-*` (scripts/lib-feature-layout.ts), while
 * the playground's pages come from two hand-maintained tables in
 * examples/react-web-csr/src — PLAYGROUND_ROUTES (routes + slugs) and
 * feature-registry.tsx (metadata + examples). Those sources drifted: card-vison
 * (May 2026) and the code-highlight packages (June 2026) landed with no route,
 * and 4e805d34 "feat(playground): unify feature routes" turned the silent gap
 * into a crash by deriving the preview URL from the table instead of from the
 * discovered short name. Nothing caught it, because the table's own test only
 * asserts uniqueness within the table.
 *
 * Three failure modes, all silent before this check:
 *   - discovered but unrouted  → `feature:preview:web <name>` crashes;
 *   - routed but unregistered  → the route opens and quietly renders
 *     core-markdown (FeaturePreview falls back to the default feature);
 *   - registered but unrouted  → feature-registry.tsx throws at module load.
 *
 * Usage: bun scripts/check-playground-routes.ts
 *   Exits 1 on any of the above, on a package listed in both tables, on a
 *   duplicate feature or a shared slug, on an exclusion without a reason, on an
 *   excluded name that matches no package, or on a registry entry naming no
 *   package.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { discoverFeaturePackages } from './lib-feature-layout';
import {
  PLAYGROUND_EXCLUDED_FEATURES,
  PLAYGROUND_ROUTES,
} from '../examples/react-web-csr/src/playground-routes';

const TAG = '[check:playground-routes]';
const REGISTRY_FILE = resolve(
  import.meta.dir,
  '..',
  'examples',
  'react-web-csr',
  'src',
  'feature-registry.tsx'
);

// Explicit `Set<string>`: both source arrays are `as const`, so inference would
// otherwise narrow these to a literal union and reject the scanned names.
const routed = new Set<string>(PLAYGROUND_ROUTES.map(route => route.feature));
const slugs = new Set<string>(PLAYGROUND_ROUTES.map(route => route.slug));
const excluded = new Set<string>(PLAYGROUND_EXCLUDED_FEATURES.map(entry => entry.feature));
const discovered = discoverFeaturePackages().map(pkg => pkg.shortName);

// A duplicate inside a hand-written table collapses in the sets above, which
// would hide it from every comparison below — so compare lengths too. A shared
// slug is the same failure seen from the other side: the later route wins in
// both lookup maps, so one feature's URL opens another feature's page.
const routeCountMismatch = routed.size !== PLAYGROUND_ROUTES.length;
const slugCountMismatch = slugs.size !== PLAYGROUND_ROUTES.length;
const exclusionCountMismatch = excluded.size !== PLAYGROUND_EXCLUDED_FEATURES.length;

const unrouted = discovered.filter(name => !routed.has(name) && !excluded.has(name));
const both = [...routed].filter(name => excluded.has(name));
const stale = [...excluded].filter(name => !discovered.includes(name));
const reasonless = PLAYGROUND_EXCLUDED_FEATURES.filter(entry => entry.reason.trim().length === 0);

// The registry is read as source rather than imported: importing it pulls React
// and all 17 feature packages (plus their runtimes) into this script.
//
// The pattern is deliberately loose — it matches `from '<pkg>'` with either
// quote style anywhere in the file, so prettier-style multi-line imports still
// resolve, and so subpath imports (`@supramark/feature-weather/runtime.web`) do
// not. Prettier enforces single quotes here, but a hand-written double-quoted
// import should still count: a missed one would surface as a confusing
// routedButUnregistered report. The cost is that a comment or string containing
// `from '@supramark/feature-x'` reads as an import; `phantomRegistered` below
// turns that into a precise failure.
//
// Type-only imports need no handling: the registry maps node types to runtime
// components, so a feature reachable only through `import type` cannot register
// one — that fails the build outright. And should a future reorganization ever
// slip past that, the symptom is still a loud routedButUnregistered pointing at
// the registry, never silently wrong data.
const registrySource = readFileSync(REGISTRY_FILE, 'utf8');
const registered = new Set(
  [...registrySource.matchAll(/from ['"]@supramark\/feature-([a-z0-9-]+)['"]/g)].map(
    match => match[1]!
  )
);
const routedButUnregistered = [...routed].filter(name => !registered.has(name));
const registeredButUnrouted = [...registered].filter(
  name => !routed.has(name) && discovered.includes(name)
);
const phantomRegistered = [...registered].filter(name => !discovered.includes(name));

const problems =
  unrouted.length +
  both.length +
  stale.length +
  reasonless.length +
  routedButUnregistered.length +
  registeredButUnrouted.length +
  phantomRegistered.length +
  (routeCountMismatch ? 1 : 0) +
  (slugCountMismatch ? 1 : 0) +
  (exclusionCountMismatch ? 1 : 0);

if (problems > 0) {
  console.error(`${TAG} FAIL: playground routes, registry, and feature packages disagree.`);
  for (const name of unrouted) {
    console.error(
      `  ${name}: discovered but neither routed nor excluded — add it to PLAYGROUND_ROUTES, or to PLAYGROUND_EXCLUDED_FEATURES with a reason.`
    );
  }
  for (const name of routedButUnregistered) {
    console.error(
      `  ${name}: routed but missing from feature-registry.tsx — visiting its page would silently render core-markdown.`
    );
  }
  for (const name of registeredButUnrouted) {
    console.error(
      `  ${name}: registered in feature-registry.tsx but not routed — that file throws at module load for it.`
    );
  }
  for (const name of phantomRegistered) {
    console.error(
      `  ${name}: named in feature-registry.tsx but no such feature package exists — the extraction matched a comment or string, or the package was deleted.`
    );
  }
  for (const name of both) {
    console.error(`  ${name}: listed in both PLAYGROUND_ROUTES and PLAYGROUND_EXCLUDED_FEATURES.`);
  }
  for (const name of stale) {
    console.error(
      `  ${name}: excluded but no such feature package exists — drop it from PLAYGROUND_EXCLUDED_FEATURES.`
    );
  }
  for (const name of reasonless.map(entry => entry.feature)) {
    console.error(
      `  ${name}: excluded without a reason — the preview CLI prints it to whoever asks.`
    );
  }
  if (routeCountMismatch) {
    console.error(
      `  PLAYGROUND_ROUTES: ${PLAYGROUND_ROUTES.length} entries but ${routed.size} distinct features — a feature is listed twice.`
    );
  }
  if (slugCountMismatch) {
    console.error(
      `  PLAYGROUND_ROUTES: ${PLAYGROUND_ROUTES.length} entries but ${slugs.size} distinct slugs — two features share a route.`
    );
  }
  if (exclusionCountMismatch) {
    console.error(
      `  PLAYGROUND_EXCLUDED_FEATURES: ${PLAYGROUND_EXCLUDED_FEATURES.length} entries but ${excluded.size} distinct features — a feature is listed twice.`
    );
  }
  console.error('');
  console.error('  Both tables live under examples/react-web-csr/src/.');
  process.exit(1);
}

console.log(
  `${TAG} OK: ${discovered.length} feature packages — ${routed.size} routed, ${excluded.size} excluded, registry in sync.`
);
