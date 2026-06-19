# Storage & TTL Strategy

This repository contains Soroban contracts that store data on-ledger. Soroban ledger entries have a storage TTL (time-to-live) measured in ledgers; if an entry’s TTL reaches zero, that entry is expired and its data is no longer available.

## Campaign Contract

### Storage Types

Campaign-related state is stored using:

- **Persistent storage** (long-lived): campaign data and per-campaign raised totals.
- **Temporary storage** (short-lived): a small “TTL bump lock” value used to avoid repeatedly extending TTL in the same short window.

### Persistent Keys

The campaign contract uses the following persistent keys:

- `CampaignCount`: global counter for allocating new campaign IDs.
- `Campaign(<id>)`: campaign tuple `(id, owner, goal, deadline, status, created_at)`.
- `Raised(<id>)`: total raised amount for the campaign.

### TTL Bumping

Active campaigns are kept alive by extending TTL for the relevant persistent keys whenever the campaign is interacted with.

- When a campaign is **registered**, the contract stores `Campaign(<id>)` and `Raised(<id>)` in persistent storage and bumps their TTL.
- When an active campaign is **read or updated**, the contract bumps TTL for `Campaign(<id>)` and `Raised(<id>)`.
- When the campaign counter is read/updated, the contract bumps TTL for `CampaignCount`.

The contract performs TTL extension via:

`env.storage().persistent().extend_ttl(key, threshold, bump_to)`

Where:

- `threshold` is the minimum remaining TTL (in ledgers) under which an extension is performed.
- `bump_to` is the target TTL (in ledgers) after extension.

Current parameters (see `contracts/campaign/src/lib.rs`):

- `CAMPAIGN_TTL_THRESHOLD_LEDGERS = 7 days`
- `CAMPAIGN_TTL_BUMP_TO_LEDGERS = 30 days`

Networks may enforce a maximum TTL for persistent entries; the runtime clamps extensions to the network’s configured maximum.

### Temporary “Bump Lock”

To reduce redundant TTL extension calls, `bump_campaign_ttl` writes a temporary key `CampaignTtlBumpLock(<id>)` containing the last ledger sequence where TTL was bumped. If the last bump is recent (within a small ledger window), the function skips extending TTL for that call.

This key is stored in temporary storage so it naturally expires and does not become part of long-lived state.
