export type AtaEndpointRef = {audience: string; endpoint_id: string};
export type AtaGraphNode = {
  app_id: string; endpoint_id: string; ata_id: string; name: string;
  description: string; critical: boolean; note_to_user?: string | null;
  additional_warnings: string[]; downstream: AtaEndpointRef[];
};
/** The API returns one complete flat graph; downstream entries are references. */
export function ataDependencies(node: AtaGraphNode, graph: AtaGraphNode[]) {
  return (node.downstream || []).map(reference => {
    const target = graph.find(candidate => candidate.app_id === reference.audience && candidate.endpoint_id === reference.endpoint_id);
    return {name: target?.name || reference.endpoint_id, ata_id: target?.ata_id || `[${reference.audience}:ata:${reference.endpoint_id}]`};
  });
}
export function ataStatus(value: {revoked_at?: string | null; expires_at: string | null; active?: boolean}, now = Date.now()) {
  if (value.revoked_at) return "Revoked";
  if (value.expires_at && Date.parse(value.expires_at) <= now) return "Expired";
  if (value.active === false) return "Unavailable";
  return "Active";
}
