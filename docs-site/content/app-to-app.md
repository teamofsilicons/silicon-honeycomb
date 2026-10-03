## Endpoint definitions and verifications

An ATA endpoint describes an action an application exposes to other applications. Configure these definitions inside the provider application's settings through `ata_endpoints`. Each has a system-wide ID such as `[fieldnotes:ata:notes.read]`, a name, endpoint path, description, criticality, metadata, optional note, warnings, and explicit ATA dependencies. Importing an OBO definition copies its description; it does not copy a user's consent or turn an ATA chain into OBO authority.

A verification authorizes a particular application chain. Manage the verifications you sign from **App to App** in the Honeycomb console's main navigation. This is a centralized page across the applications you can currently manage, filtered to your signing identity. Endpoint configuration remains within each application. Changing your selected organization changes the management authority available to the page.

## Create a verification

1. Choose the originating application you manage, then the applications and ATA endpoints that should participate.
2. Set the verification lifetime: at least one hour, or never. The default is never. Separately choose access-token validity from one minute to 24 hours; the default is 30 minutes.
3. Preview the complete chain. Dependencies can add both applications and endpoints. Review those additions before creating the verification.
4. Create using the reviewed graph version. IAM records the signing Carbon or Silicon from the authenticated manager; callers cannot choose a different signer.
5. Store the returned refresh credential securely on the originating backend. It is shown during creation and is not returned by ordinary list operations.

An expired preview or changed dependency graph needs another review. A failed or unavailable application in the centralized list is shown as an incomplete result, not silently treated as having no verifications.

## CLI and API

```sh
# Your signed verifications across applications you can manage
honeycomb apps ata list --json

# All verifications for one application you manage
honeycomb apps ata list fieldnotes --json

# Review a request, then create from the reviewed request including graph_version
honeycomb apps ata preview fieldnotes request.json --json
honeycomb --idempotency-key UUID apps ata create fieldnotes reviewed-request.json --json

# Revoke the verification and its access/refresh credentials
honeycomb --idempotency-key ANOTHER_UUID apps ata revoke fieldnotes VERIFICATION_ID
```

Keep the same UUID when retrying the same mutation after an uncertain response. The create response contains a credential, so capture it in protected storage rather than a shared log. See the [CLI reference](/cli-reference/#honeycomb-apps-ata) for exact arguments.

The centralized list uses `GET /api/v1/ata-verifications`. Creation, preview, application-wide listing, and revocation use `/api/v1/apps/{app}/ata-verifications`. These application-scoped API paths bind management authority even though the website presents one unified workspace. Management currently runs through the production console.

## Runtime verification

The originating backend exchanges its refresh credential for short-lived access proof. Every receiving application authenticates itself to IAM and verifies the requested ATA endpoint, calling application, and chain before acting. Each receiver also enforces its own resource policy. Verification failure must deny the operation, regardless of whether the cause is an invalid proof or a missing endpoint permission.

ATA represents application authority throughout the chain. It cannot become OBO later, and the recorded signer does not become the user on whose behalf the runtime request acts. Revocation invalidates the verification's credentials. Follow the [IAM ATA client contract](https://docs.iam.teamofsilicons.com/client/ata/) for exchange, refresh, and receiver verification.
