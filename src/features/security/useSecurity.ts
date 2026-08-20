// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Security report data.
 *
 * Two things worth stating about the caching, because both are security decisions rather
 * than performance ones:
 *
 * **The report is fetched on demand, not on mount of the app.** It decrypts every login's
 * password to score them, so running it because a sidebar row exists would decrypt the
 * whole vault on launch. `enabled` gates it on the surface actually being open.
 *
 * **`gcTime: 0`, like every other query.** The report carries no passwords, but it does
 * carry a list of which items are weak and which are breached — an inventory of exactly
 * where the vault is soft. §4.9 says locking tears down decrypted caches, and this is one.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { securityBreachCheck, securityReportRun } from '../../ipc';
import type { BreachCheckDto, SecurityReportDto } from '../../ipc';

const LOCAL = {
  gcTime: 0,
  staleTime: 0,
  refetchOnWindowFocus: false,
  retry: false,
} as const;

/** Query key for the report. */
export const reportKey = ['security-report'] as const;

/**
 * Run the report.
 *
 * Makes no network request: Rust hands it a cache-only breach source, so AC14's zero
 * requests is structural rather than a promise this hook keeps.
 */
export function useSecurityReport(enabled: boolean) {
  return useQuery<SecurityReportDto>({
    queryKey: reportKey,
    queryFn: () => securityReportRun(),
    enabled,
    ...LOCAL,
  });
}

/**
 * Read the report the security surface has already run, without running one.
 *
 * `enabled: false` subscribes to the cache entry rather than fetching it, so the item
 * list's risk dots and the detail pane's strength band light up once the user has opened
 * the report — and never cause the whole vault to be decrypted on launch to get them.
 * Until then there is no band, which is §7.4's *"offline is 'not checked', never
 * 'safe'"* applied to the surfaces that only borrow the result.
 */
export function useCachedSecurityReport() {
  return useQuery<SecurityReportDto>({
    queryKey: reportKey,
    queryFn: () => securityReportRun(),
    enabled: false,
    ...LOCAL,
    // A cache entry the report surface wrote is the whole point; dropping it the moment
    // that surface unmounts would take the dots away again.
    gcTime: Infinity,
  });
}

/**
 * Refresh the HIBP range cache, then re-run the report.
 *
 * The only thing in the app that reaches HIBP. It enforces §7.4's once-per-24-hours
 * cadence in Rust, so calling it inside the interval is a no-op that reports
 * `ran: false` rather than an error — which is why the button stays enabled-looking but
 * the command decides.
 */
export function useBreachCheck() {
  const client = useQueryClient();
  return useMutation<BreachCheckDto>({
    mutationFn: () => securityBreachCheck(),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: reportKey });
    },
  });
}
