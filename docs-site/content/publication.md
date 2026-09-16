## Start private
New applications are private. Upload at least one valid six-target CLI release and supply the required registration details before requesting publication.
```sh
honeycomb apps get 'my-org>my-app'
honeycomb publication request 'my-org>my-app' --revision 1 --message 'Describe the application and why its requested access is needed.'
honeycomb publication get 'my-org>my-app'
```
Replace the revision with current state. Submission creates durable review work, not immediate public access.

## Review stages
1. Provider administrators review the critical external scopes belonging to their own applications.
2. IAM reviewers exercise IAM's specific review authority where required.
3. Honeycomb validators review the package and publication after required provider approvals.
4. IAM accepts activation and Honeycomb reconciles archive visibility before publication is complete.

Requested access, approval, effective permissions, and public archive access are distinct states. The current production integration still has [contract gaps](/availability/); a pending operation must remain pending until the responsible service confirms it.

## Participate as a reviewer
```sh
honeycomb publication inbox
honeycomb publication review REQUEST_ID 'provider-org>provider-app'
honeycomb publication review-reply REQUEST_ID 'provider-org>provider-app' --message 'Please explain why this endpoint is needed.'
honeycomb publication decide REQUEST_ID 'provider-org>provider-app' approve --revision 1 --reason 'Access reviewed.'
```
A denial requires a reason. Being an organization admin does not grant global validation rights. Use current review state/revisions, not an application's release version.

## Reply or resume
```sh
honeycomb publication reply 'my-org>my-app' --message 'Additional justification for the requested access.'
honeycomb publication retry-plan REQUEST_ID --revision 1
honeycomb publication activate REQUEST_ID --revision 1
```
Use the request's current revision and only perform actions your role permits. The console provides the same discussions under **Review requests**. [Operations and retries](/operations/) explains recovery without duplicate work.
