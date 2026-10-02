export type DelegationEndpoint = {endpoint_id: string; name?: string; description?: string; path: string; critical: boolean; metadata?: unknown; note_to_user?: string | null; additional_warnings?: string[]; downstream?: {audience: string; endpoint_id: string}[]; enabled?: boolean; ttl_seconds?: number; [key: string]: unknown};
/** Import a definition without translating any user's grant or OBO dependencies. */
export function importOboDefinition(source: DelegationEndpoint): DelegationEndpoint {
  return {endpoint_id: source.endpoint_id, name: source.name || source.endpoint_id, description: source.description || "", path: source.path, critical: source.critical, metadata: structuredClone(source.metadata ?? {}), note_to_user: source.note_to_user || null, additional_warnings: [...(source.additional_warnings || [])], downstream: [], enabled: source.enabled !== false};
}
