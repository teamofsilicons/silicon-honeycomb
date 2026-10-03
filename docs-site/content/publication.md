## Public approval by default
New production applications automatically request public approval when IAM accepts their configuration and at least one valid six-target production CLI release has been uploaded or promoted from development. A development release alone does not trigger public approval. The request uses the application description as its initial justification; managers can add context in its discussion. Effective visibility stays private until all required approvals pass.

Release channel and application visibility are separate choices. An upload must specify production or development, and those channels retain independent version histories. A development release can be promoted by choosing a production version; this creates a new production release and preserves its development source. See [Upload an application](/upload-an-app/) for console and CLI instructions. Promotion does not bypass the application's visibility or scope approvals.

To keep an application private, choose private in the console or set `"visibility": "private"` in the application JSON. Updates preserve the existing preference when omitted, and test-environment applications never auto-submit production reviews. Repeated configuration/upload operations reuse the same revision’s review request.

For an application previously kept private, you can explicitly request publication:
```sh
honeycomb apps get 'my-app'
honeycomb publication request 'my-app' --revision 1 --message 'Describe the application and why its requested access is needed.'
honeycomb publication get 'my-app'
```
Replace the revision with current state. Submission creates durable review work, not immediate public access.

## Review stages
1. Provider administrators review the critical external scopes belonging to their own applications.
2. IAM reviewers exercise IAM's specific review authority where required.
3. Honeycomb validators review the package and publication after required provider approvals.
4. IAM accepts activation and Honeycomb reconciles archive visibility before publication is complete.

Requested access, approval, effective permissions, and public archive access are distinct states. A pending operation remains pending until the responsible service confirms it. All application review discussions and notification emails belong to Honeycomb, including reviews of critical IAM permissions. IAM validates reviewer authority and enforces the resulting permission decisions. User login consent and personal OBO consent remain in IAM.

## Participate as a reviewer
```sh
honeycomb publication inbox
honeycomb publication review REQUEST_ID 'provider-app'
honeycomb publication review-reply REQUEST_ID 'provider-app' --message 'Please explain why this endpoint is needed.'
honeycomb publication decide REQUEST_ID 'provider-app' approve --revision 1 --reason 'Access reviewed.'
```
A denial requires a reason. Being an organization admin does not grant global validation rights. Use current review state/revisions, not an application's release version.

## Reply or resume
```sh
honeycomb publication reply 'my-app' --message 'Additional justification for the requested access.'
honeycomb publication retry-plan REQUEST_ID --revision 1
honeycomb publication activate REQUEST_ID --revision 1
```
Use the request's current revision and only perform actions your role permits. The console provides the same discussions under **Received requests** and **Sent requests**, each with its own discussion page. Migrated legacy IAM email links retain their original request ID and open the corresponding Honeycomb discussion after current access is checked. [Operations and retries](/operations/) explains recovery without duplicate work.
