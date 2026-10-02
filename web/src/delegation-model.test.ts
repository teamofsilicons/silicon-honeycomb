import assert from "node:assert/strict";
import test from "node:test";
import {importOboDefinition} from "./delegation-model";
test("importing OBO into ATA copies endpoint disclosure but never delegation or token policy", () => {
  const source = {endpoint_id: "tts", name: "Text to speech", description: "Generate audio", path: "/tts", critical: false, ttl_seconds: 300, downstream: [{audience: "briefcase", endpoint_id: "upload"}], metadata: {format: "mp3"}, additional_warnings: ["uses_credits"], note_to_user: "Uses your voice credits"};
  const value = importOboDefinition(source);
  assert.deepEqual(value.downstream, []); assert.equal(value.ttl_seconds, undefined); assert.equal(value.name, source.name);
  assert.deepEqual(value.metadata, source.metadata); assert.deepEqual(value.additional_warnings, source.additional_warnings);
  (value.metadata as {format: string}).format = "wav"; value.additional_warnings!.push("stores_data");
  assert.equal(source.metadata.format, "mp3"); assert.deepEqual(source.additional_warnings, ["uses_credits"]);
});

import {ataDependencies, ataStatus, type AtaGraphNode} from "./ata-model";
test("ATA dependency references resolve against the complete graph without inventing endpoint records", () => {
  const graph: AtaGraphNode[] = [
    {app_id:"waveform", endpoint_id:"speak", ata_id:"[waveform:ata:speak]", name:"Generate speech", description:"Generate audio", critical:false, additional_warnings:[], downstream:[{audience:"briefcase",endpoint_id:"store"}]},
    {app_id:"briefcase", endpoint_id:"store", ata_id:"[briefcase:ata:store]", name:"Store audio", description:"Store generated audio", critical:true, additional_warnings:["stores_data"], downstream:[]},
  ];
  assert.deepEqual(ataDependencies(graph[0], graph), [{name:"Store audio",ata_id:"[briefcase:ata:store]"}]);
  assert.deepEqual(ataDependencies(graph[1], graph), []);
  // A partial/error response must never fabricate an undefined app identity.
  assert.deepEqual(ataDependencies(graph[0], [graph[0]]), [{name:"store",ata_id:"[briefcase:ata:store]"}]);
});
test("ATA status respects live invalidation even for unexpired never-expiring records", () => {
  const now = Date.parse("2026-10-02T12:00:00Z");
  assert.equal(ataStatus({expires_at:null,active:false}, now), "Unavailable");
  assert.equal(ataStatus({expires_at:"2027-01-01T00:00:00Z",active:false}, now), "Unavailable");
  assert.equal(ataStatus({expires_at:null,active:true}, now), "Active");
  assert.equal(ataStatus({expires_at:"2026-01-01T00:00:00Z",active:false}, now), "Expired");
  assert.equal(ataStatus({expires_at:null,revoked_at:"2026-10-01T00:00:00Z",active:false}, now), "Revoked");
});
