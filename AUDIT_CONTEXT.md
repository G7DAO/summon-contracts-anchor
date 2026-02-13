# Deep Audit Context — Summon Rewards (Anchor/Solana)

**Program ID:** `3XVtDWuE5Pgbu1bG2FNYBfkj51sdXQDXFg93rTnKUoJm`
**Framework:** Anchor 0.30.1, Token-2022, SPL Token
**Date:** 2026-02-13

---

## Phase 1 — Initial Orientation

### 1.1 System Overview

**Summon Rewards** is a Solana/Anchor program that is a port of an EVM-based reward system (Rewards.sol / Treasury.sol / RewardsState.sol / AccessToken.sol). It implements a "reward token" system where:

1. **Managers** create "reward tokens" — each representing a reward package containing a combination of SOL, SPL tokens, and/or NFTs.
2. **Users** mint "access tokens" (soulbound Token-2022 tokens with NonTransferable extension) that grant them the right to claim a reward package.
3. **Users** burn their access tokens to claim the underlying rewards from the treasury.

### 1.2 Actors

| Actor | Role Key in Config | Capabilities |
|---|---|---|
| **Admin** | `config.admin` | Update all roles (admin, manager, minter, dev_config) |
| **Manager** | `config.manager` | Create reward tokens, manage treasury (whitelist/deposit/withdraw), pause/unpause, manage reservations, update token settings |
| **Minter** | `config.minter` | Admin-mint access tokens to users |
| **DevConfig** | `config.dev_config` | Add/remove whitelist signers for Ed25519 signature verification |
| **User** | Any signer | Mint with signature, claim rewards, deposit to treasury, burn access tokens |
| **Depositor** | Any signer | Deposit SPL tokens or NFTs to treasury (if token is whitelisted) |

### 1.3 Entrypoints (21 instructions)

| Category | Instruction | Access Control |
|---|---|---|
| **Admin** | `initialize` | Deployer (becomes admin) |
| | `update_roles` | Admin |
| | `pause` / `unpause` | Manager |
| | `add_whitelist_signer` / `remove_whitelist_signer` | DevConfig |
| **Treasury** | `whitelist_token` / `remove_token_from_whitelist` | Manager |
| | `deposit_to_treasury` / `deposit_nft_to_treasury` | Anyone (if token whitelisted) |
| | `withdraw_unreserved_treasury` / `withdraw_unreserved_nft` | Manager |
| **Reservation** | `init_token_reservation` / `init_nft_reservation` | Manager |
| **Reward Token** | `create_reward_token` | Manager |
| | `update_token_mint_paused` / `update_claim_paused` | Manager |
| | `increase_reward_supply` | Manager |
| | `update_token_uri` | Manager |
| **Access Token** | `create_access_token_mint` | Manager |
| | `mint_access_token` | Minter |
| | `burn_access_token` | Token Holder |
| **Minting** | `admin_mint` | Minter |
| | `mint_with_signature` | Any user (with valid Ed25519 signature) |
| **Claiming** | `claim_reward` | Any user (who holds access tokens) |
| | `admin_claim_reward` | Manager |

### 1.4 PDA Map

| PDA | Seeds | Account Type | Purpose |
|---|---|---|---|
| Config | `["config"]` | `RewardsConfig` | Global config: roles, pause state, token count |
| Whitelist Signers | `["signers", config]` | `WhitelistSigners` | Ed25519 signer whitelist |
| Token Whitelist | `["whitelist", config]` | `TokenWhitelist` | Whitelisted treasury tokens |
| Treasury State | `["treasury", config]` | `TreasuryState` | Treasury PDA (authority for ATAs) |
| Reward Token | `["reward_token", config, token_id]` | `RewardTokenState` | Per-reward-token metadata & rewards |
| Access Mint | `["access_mint", config, token_id]` | Token-2022 Mint | Soulbound access token mint |
| Token Reservation | `["reserve", config, mint]` | `TokenReservation` | Reserved SPL amount tracking |
| NFT Reservation | `["nft_reserve", config, nft_mint]` | `NftReservation` | Per-NFT reservation flag |
| User Nonce | `["nonce", config, user, nonce]` | `UserNonce` | Replay protection for signature mints |

---

## Phase 2 — Ultra-Granular Function Analysis

### 2a: State Structs

#### `RewardsConfig` (`state/config.rs`)

**Purpose:** Singleton global configuration PDA storing role assignments, global pause flag, and reward token counter. Acts as the central access control registry.

**Fields & Assumptions:**
- `admin` (Pubkey): DEFAULT_ADMIN equivalent. Can change all roles including itself. **Assumption:** Admin is a trusted party; compromise allows full takeover.
- `manager` (Pubkey): MANAGER_ROLE. Most powerful operational role — creates tokens, manages treasury, pauses. **Assumption:** Manager operates honestly.
- `minter` (Pubkey): MINTER_ROLE. Can admin-mint access tokens. **Assumption:** Minter won't inflate supply beyond expectations.
- `dev_config` (Pubkey): DEV_CONFIG_ROLE. Manages Ed25519 signer whitelist. **Assumption:** DevConfig won't add malicious signers.
- `is_paused` (bool): Global pause flag. When true, blocks minting and claiming.
- `reward_token_count` (u64): Monotonically increasing counter. **Not used as token_id** — token_id is user-supplied.
- `bump` (u8): PDA bump seed for `["config"]`.

**LEN calculation:** 8 + 32*4 + 1 + 8 + 1 = 146 bytes. Correct.

**Invariants:**
1. Admin, manager, minter, dev_config are never `Pubkey::default()` after initialization.
2. `reward_token_count` only increases (via `create_reward_token`).
3. Only admin can modify role assignments.

#### `UserNonce` (`state/nonce.rs`)

**Purpose:** Per-user per-nonce PDA for signature replay protection. Existence of the PDA (via `init` constraint) guarantees one-time use.

**Fields:**
- `used` (bool): Always set to `true` upon creation. The `init` constraint is the primary replay protection — PDA already exists = account init fails.
- `bump` (u8): PDA bump.

**Invariants:**
1. Once created, a UserNonce PDA is never deleted or reset.
2. The `used` field is redundant safety — the `init` constraint is the true guard.

#### `TokenReservation` (`state/reservation.rs`)

**Purpose:** Tracks the total reserved amount of a specific SPL token mint across all reward tokens. Prevents treasury withdrawal of committed funds.

**Fields:**
- `mint` (Pubkey): The SPL token mint this reservation tracks.
- `reserved_amount` (u64): Total reserved across all reward tokens. Increased on `create_reward_token`/`increase_reward_supply`, decreased on `claim_reward`.
- `bump` (u8): PDA bump.

**Invariants:**
1. `reserved_amount` <= actual treasury token account balance (enforced at creation/supply increase).
2. `reserved_amount` decreases by exactly `reward.amount` per claim.

#### `NftReservation` (`state/reservation.rs`)

**Purpose:** Tracks whether a specific NFT is reserved for a reward.

**Fields:**
- `nft_mint` (Pubkey): The NFT mint.
- `is_reserved` (bool): True when assigned to a reward token, false when claimed.
- `bump` (u8): PDA bump.

**Invariants:**
1. An NFT with `is_reserved == true` cannot be withdrawn from treasury.
2. Reservation is released upon claim.

#### `RewardTokenState` (`state/reward_token.rs`)

**Purpose:** Per-reward-token state. Holds the token's metadata, supply tracking, pause flags, access token mint reference, and the reward package definition.

**Key Fields:**
- `token_id` (u64): User-supplied, must be > 0. Uniqueness enforced by PDA seed `["reward_token", config, token_id]`.
- `token_uri` (String): Metadata URI, max 200 chars.
- `max_supply` (u64): Maximum mintable access tokens.
- `current_supply` (u64): Current minted count. Increases on mint, never decreases (no burn decrement).
- `is_mint_paused` / `is_claim_paused` (bool): Per-token pause flags.
- `access_token_mint` (Pubkey): Token-2022 mint for soulbound access tokens. Default until `create_access_token_mint` is called.
- `rewards` (Vec\<RewardEntry\>): The reward package — list of reward entries.
- `bump` (u8): PDA bump.

**RewardEntry sub-struct:**
- `reward_type`: Sol | SplToken | Nft
- `amount`: Per-claim amount (lamports for Sol, base units for SPL, NFT count for Nft)
- `token_mint`: Optional — the SPL/NFT collection mint
- `nft_mints`: Vec of specific NFT mints for Nft type
- `nft_current_index`: Rolling index tracking which NFT to distribute next

**Invariants:**
1. `current_supply <= max_supply` (enforced at mint time).
2. For NFT rewards: `nft_mints.len() == amount * max_supply` at creation.
3. `nft_current_index` advances by `amount` per claim. Never exceeds `nft_mints.len()`.

#### `WhitelistSigners` (`state/signers.rs`)

**Purpose:** Stores authorized Ed25519 signer public keys for signature-based minting.

**Fields:**
- `signers` (Vec\<Pubkey\>): List of authorized signers. Max space allocated for 10.
- `bump` (u8): PDA bump.

**Invariants:**
1. No duplicate signers.
2. No `Pubkey::default()` in the list.

#### `TreasuryState` (`state/treasury.rs`)

**Purpose:** Minimal PDA whose existence enables it to act as authority over treasury-owned ATAs. Also holds SOL (lamports) directly for SOL rewards.

**Fields:**
- `bump` (u8): PDA bump.

**Invariants:**
1. Treasury PDA is the owner of all treasury token accounts.
2. SOL lamports in treasury PDA fund SOL rewards.

#### `TokenWhitelist` (`state/whitelist.rs`)

**Purpose:** Stores whitelist of token mints that can be deposited to treasury.

**Fields:**
- `tokens` (Vec\<WhitelistEntry\>): List of entries. Max space allocated for 50.
- `bump` (u8): PDA bump.

**WhitelistEntry:**
- `mint` (Pubkey): Token mint.
- `reward_type` (RewardType): Sol | SplToken | Nft.
- `is_active` (bool): Soft-delete flag.

**Invariants:**
1. No duplicate active entries for the same mint.
2. Removal uses `swap_remove` — ordering is not preserved.

---

### 2b: `initialize` (`instructions/initialize.rs`)

**Purpose:** One-time initialization of the program's global state. Creates all four singleton PDAs: config, whitelist signers, token whitelist, and treasury state. Sets the deployer as admin and assigns manager, minter, and dev_config roles.

**Inputs & Assumptions:**
- `admin` (Signer, mut): The deployer. Pays rent for all PDAs. Becomes `config.admin`.
- `manager`, `minter`, `dev_config` (Pubkey args): Role assignments. Must be non-default.
- **Assumption:** Called exactly once. The `init` constraint on config PDA with seeds `["config"]` guarantees this — second call fails with "already initialized."

**Outputs & Effects:**
- Creates 4 PDAs (config, whitelist_signers, token_whitelist, treasury_state).
- Emits `ProgramInitialized` event.
- No external CPI calls.

**Block-by-Block Analysis:**

1. **Lines 65-76: Zero-address validation**
   - Validates manager, minter, dev_config are not `Pubkey::default()`.
   - **Why here:** Prevents setting roles to the system default (effectively a "null" address), which would brick role-gated instructions.
   - **Note:** Does NOT validate that admin itself is non-default. Admin is derived from the signer, so it can't be default (a signer must have a valid keypair). Safe.

2. **Lines 78-86: Config initialization**
   - Sets admin to the signer's pubkey.
   - Sets roles, pause=false, token_count=0, bump.
   - **Invariant established:** All roles are non-default. Program starts unpaused.

3. **Lines 88-100: Subsidiary PDA initialization**
   - Signers and whitelist start with empty vecs. Treasury just stores bump.
   - **Invariant established:** No signers, no whitelisted tokens, empty treasury.

**Cross-Function Dependencies:**
- Every other instruction depends on config existing.
- Config PDA key is used in seeds for ALL other PDAs.

**Invariants Established:**
1. Config is a singleton (PDA seed guarantee).
2. All roles are non-default after init.
3. Program starts unpaused with zero tokens.

---

### 2c: Admin Instructions (`instructions/admin.rs`)

#### `update_roles_handler`

**Purpose:** Allows the admin to change any role (admin, manager, minter, dev_config).

**Inputs & Assumptions:**
- `admin` (Signer): Must be current `config.admin` (enforced by account constraint).
- `role` (RoleType enum): Which role to update.
- `new_address` (Pubkey): Must be non-default.

**Block-by-Block Analysis:**

1. **Lines 72-75: Zero-address validation** — Prevents bricking a role.
2. **Lines 78-99: Role update match** — Updates the specific role, captures old address for event.
3. **Lines 101-105: Event emission** — Emits `RoleUpdated`.

**Key Observations:**
- Admin can transfer admin to another key. **5 Whys: Why can admin change itself?** To support admin key rotation. **Risk:** If admin sets itself to a compromised key, the entire program is compromised with no recovery path. **No timelock or multi-sig is enforced.**
- Admin can set manager/minter/dev_config to the same address, concentrating power.

**Invariants:**
1. Only current admin can update roles.
2. New role address is always non-default.

#### `pause_handler` / `unpause_handler`

**Purpose:** Global pause toggle. Only manager can invoke.

**Analysis:**
- Simple flag set. No additional validation.
- **Note:** No per-event check — idempotent (pausing when already paused is fine).
- **Constraint:** `config.manager == manager.key()` in `PauseUnpause` struct.

#### `add_whitelist_signer_handler`

**Purpose:** Add an Ed25519 signer to the whitelist.

**Analysis:**
1. **Zero-address check** on signer_to_add.
2. **Duplicate check**: `!signers.contains(&signer_to_add)`. Linear scan — O(n) but n <= 10.
3. **Push** to vec.
- **Space concern:** Space allocated for 10 signers. If 10 are added, the 11th would fail at serialization (account data too small). No explicit check for max capacity.

#### `remove_whitelist_signer_handler`

**Purpose:** Remove an Ed25519 signer from the whitelist.

**Analysis:**
1. **Zero-address check**.
2. **Position lookup** — fails if not found.
3. **swap_remove** — O(1) removal, changes ordering. This is fine since ordering doesn't matter for whitelist.

---

### 2d: Treasury Instructions (`instructions/treasury.rs`)

#### `whitelist_token_handler`

**Purpose:** Add a token mint to the treasury whitelist, enabling deposits of that token.

**Inputs:** manager (Signer), mint (Pubkey), reward_type (RewardType).

**Block-by-Block:**

1. **Line 254-257: Zero-address check** on mint.
2. **Lines 261-269: Duplicate check** — Iterates over `whitelist.tokens`, checking if any entry has same mint AND is active.
   - **Observation:** After `swap_remove` in `remove_token_from_whitelist`, an inactive entry is deleted entirely (not soft-deleted). The `is_active` field on `WhitelistEntry` is set to `true` at creation but is never set to `false` anywhere in the code. The `is_active` field appears vestigial from an earlier soft-delete design. All active entries always have `is_active == true`.
3. **Lines 271-275: Push new entry** — Always sets `is_active: true`.

**Invariants:**
1. No duplicate active entries for the same mint.
2. Only whitelisted tokens can be deposited.

#### `remove_token_from_whitelist_handler`

**Purpose:** Remove a token from the whitelist, preventing future deposits.

**Block-by-Block:**

1. **Lines 293-297: Find entry** — Searches for active entry with matching mint.
2. **Lines 299: Capture reward_type** before mutable borrow.
3. **Lines 302-322: SPL reservation check via remaining_accounts**
   - If the token is `SplToken`, the caller MAY pass a `TokenReservation` PDA in remaining_accounts.
   - **Critical observation:** The check is `if !remaining.is_empty()` — this means the reservation check is **optional**. If the caller does NOT pass remaining_accounts, the check is SKIPPED entirely. This allows removing a whitelisted SPL token that has active reservations.
   - **Further:** Even when remaining_accounts IS provided, the code checks `if reservation_ai.key() == expected_pda` — and if it DOESN'T match, it silently continues without error. So a caller could pass a random account and bypass the check.
   - **No check for Nft type** — NFT reservations are not checked at all during whitelist removal.
4. **Line 325: swap_remove** — Physical removal.

**Risk considerations:**
- Manager can remove a whitelisted token that has active reservations, potentially breaking the invariant that reserved tokens are protected.

#### `deposit_to_treasury_handler`

**Purpose:** Deposit SPL tokens to treasury. Anyone can call.

**Block-by-Block:**

1. **Line 335: Amount > 0 check.**
2. **Lines 338-345: Whitelist check** — Verifies the mint is whitelisted as `SplToken` type and active.
3. **Lines 348-358: SPL token transfer CPI** — Standard Token program transfer from depositor to treasury ATA.
   - **Note:** No access control — anyone can deposit if token is whitelisted.
   - **Note:** No pause check — deposits work even when program is paused.

**Invariants:**
1. Only whitelisted SplToken mints can be deposited.
2. Transfer uses depositor's authority (standard user-signed CPI).

#### `withdraw_unreserved_treasury_handler`

**Purpose:** Manager withdraws SPL tokens from treasury that are not reserved.

**Block-by-Block:**

1. **Line 374-375: Read balance and reserved.**
2. **Line 377: Require balance > reserved** — Strict greater-than, not >=. This means if balance == reserved, withdrawal fails. The intent is to only withdraw the unreserved surplus.
   - **Note:** Uses `>` not `>=`. When balance equals reserved, `withdraw_amount` would be 0, and `balance > reserved` fails. This is correct — prevents zero-amount withdrawal.
3. **Lines 379-381: Calculate withdraw_amount** — checked_sub for safety.
4. **Lines 383-399: CPI transfer with treasury PDA signer** — Treasury PDA signs the transfer.

**Invariants:**
1. Can never withdraw below reserved amount.
2. Only manager can withdraw.

#### `deposit_nft_to_treasury_handler`

**Purpose:** Deposit an NFT to treasury.

**Analysis:**
- Verifies NFT mint is whitelisted as `Nft` type.
- Account constraint: `nft_mint.decimals == 0` and `depositor_token_account.amount == 1`.
- Transfers exactly 1 token.
- **No pause check.**
- **No access control** beyond token ownership.

#### `withdraw_unreserved_nft_handler`

**Purpose:** Manager withdraws unreserved NFTs from treasury.

**Analysis:**
- NftReservation constraint: `!nft_reservation.is_reserved` — ensures NFT is not reserved.
- Treasury token account constraint: `amount == 1` — ensures NFT is in treasury.
- Transfers exactly 1 token with treasury PDA signer.

---

### 2e: Reservation Instructions (`instructions/reservation.rs`)

#### `init_token_reservation_handler`

**Purpose:** Create a `TokenReservation` PDA for a specific SPL token mint. Must be done before `create_reward_token` references that mint.

**Analysis:**
- Manager-only (enforced by account constraint).
- PDA seeds: `["reserve", config, mint]` — one reservation per mint.
- Initializes `reserved_amount = 0`.
- Zero-address check on mint.
- The `init` constraint prevents double-creation.

#### `init_nft_reservation_handler`

**Purpose:** Create an `NftReservation` PDA for a specific NFT mint.

**Analysis:**
- Manager-only.
- PDA seeds: `["nft_reserve", config, nft_mint]` — one per NFT.
- Initializes `is_reserved = false`.
- Zero-address check on nft_mint.

**Cross-function dependency:** These must be called before `create_reward_token` for any reward referencing SPL or NFT mints. The `create_reward_token` handler deserializes these PDAs from remaining_accounts.

---

### 2f: Reward Token Instructions (`instructions/reward_token.rs`)

#### `create_reward_token_handler`

**Purpose:** Create a new reward token defining a reward package. Validates inputs, verifies treasury has sufficient funds, and reserves amounts in TokenReservation/NftReservation PDAs.

**Inputs:**
- `token_id` (u64): User-supplied ID, must be > 0.
- `token_uri` (String): Metadata URI, non-empty, max 200 chars.
- `max_supply` (u64): Must be > 0.
- `rewards` (Vec\<RewardEntry\>): The reward package definition. Non-empty.
- remaining_accounts: Reservation PDAs and treasury token accounts.

**Block-by-Block Analysis:**

1. **Lines 137-141: Input validation**
   - max_supply > 0, token_id > 0, non-empty URI <= 200 chars, non-empty rewards.
   - **Note:** `token_id` uniqueness is enforced by the PDA init constraint — if two reward tokens have the same token_id, the second init fails.

2. **Lines 144-171: Per-reward validation loop**
   - **Sol:** amount > 0.
   - **SplToken:** token_mint must be Some, amount > 0.
   - **Nft:** token_mint must be Some, `nft_mints.len() == amount * max_supply` (using u128 for overflow-safe multiplication).
   - **Observation on NFT validation:** `token_mint` is required (via `AddressIsZero` error) but it's unclear what it represents for NFTs since individual NFT mints are in `nft_mints` vec. Likely refers to the collection mint or is unused metadata.

3. **Lines 173-285: Remaining accounts processing**
   - **Sol (lines 181-185):** No remaining accounts consumed. SOL validation is deferred to claim time. **5 Whys: Why deferred?** Because SOL lamports can come and go from the treasury PDA at any time, and there's no SPL-token-like reservation mechanism for lamports. **Risk:** If treasury doesn't have enough SOL at claim time, claim fails.

   - **SplToken (lines 186-243):**
     - Consumes 2 remaining accounts: treasury_token_account, token_reservation.
     - Deserializes and validates treasury token account (owner == treasury, mint matches).
     - Deserializes token reservation and verifies PDA seeds.
     - Computes `total_amount = reward.amount * max_supply` (checked_mul).
     - Computes `new_reserved = reservation.reserved_amount + total_amount` (checked_add).
     - Verifies `treasury_token.amount >= new_reserved`.
     - Updates reservation and calls `exit()` to serialize back.
     - **Key: `exit()` is used for remaining_accounts** because they're not in the `Accounts` struct — Anchor won't auto-serialize them.

   - **Nft (lines 245-284):**
     - For each NFT mint in `nft_mints`, consumes 1 remaining account (nft_reservation PDA).
     - Verifies PDA seeds match.
     - Checks `!is_reserved` (not already reserved).
     - Sets `is_reserved = true` and calls `exit()`.

4. **Lines 287-297: Initialize RewardTokenState**
   - Sets all fields. `access_token_mint = Pubkey::default()` — must call `create_access_token_mint` separately.
   - **Stores rewards vec directly** — means the reward entries including `nft_current_index = 0` are set at creation.

5. **Lines 299-304: Increment config.reward_token_count**
   - Checked_add for overflow.

**Invariants established:**
1. For SplToken rewards: `treasury_balance >= sum(all reservations for that mint)`.
2. For Nft rewards: Each NFT mint is reserved exactly once.
3. `nft_mints.len() == amount * max_supply` ensures exactly enough NFTs for all possible claims.

**Assumptions:**
1. Treasury token accounts and reservation PDAs passed via remaining_accounts are correct (verified via PDA derivation and ownership checks).
2. SOL rewards are not reserved — relies on manager to pre-fund and maintain sufficient SOL.
3. The `token_mint` field in NFT RewardEntry is validated as non-default but its value isn't verified against any on-chain mint account.
4. No check that SPL token mints referenced in rewards are whitelisted. The whitelist is only for treasury deposits, not reward definitions.
5. NFT mints in `nft_mints` are not verified to actually be in the treasury. Only the reservation PDA existence and non-reserved status is checked.

#### `increase_reward_supply_handler`

**Purpose:** Increase max_supply of an existing reward token.

**Block-by-Block:**

1. **Line 361: additional_supply > 0.**
2. **Lines 363-367: Compute new_supply** — checked_add.
3. **Lines 370-437: Update SPL reservations** — Same pattern as create: iterates rewards, increases reservations by `reward.amount * additional_supply`.
   - **NFT handling (lines 429-436):** Explicitly skipped — "NFT supply increase requires providing new NFT mints" — left as a TODO. This means `increase_reward_supply` silently succeeds for reward tokens that include NFT rewards but **doesn't add new NFT mints**. The max_supply is increased, but when claims reach the end of the existing `nft_mints` vec, they'll fail with `InsufficientBalance`.
4. **Line 439: Update max_supply.**

**Risk consideration:** If a reward token has both SPL and NFT rewards, calling `increase_reward_supply` will correctly increase SPL reservations but leave NFT rewards under-provisioned. This creates an inconsistent state where `max_supply` suggests more claims are possible than actually are.

#### `update_token_mint_paused_handler` / `update_claim_paused_handler`

**Purpose:** Toggle per-token mint/claim pause flags. Manager-only. Straightforward flag sets.

#### `update_token_uri_handler`

**Purpose:** Update token URI. Manager-only. Validates non-empty, max 200 chars.

---

### 2g: Access Token Instructions (`instructions/access_token.rs`)

#### `create_access_token_mint_handler`

**Purpose:** Create a Token-2022 mint with NonTransferable extension (soulbound) for a specific reward token. This mint is used to issue access tokens to users.

**Inputs & Assumptions:**
- Manager-only (account constraint).
- `reward_token_state.access_token_mint == Pubkey::default()` — ensures one-time creation per reward token.
- Access mint PDA: `["access_mint", config, token_id]`.

**Block-by-Block:**

1. **Lines 133-142: PDA signer seeds** for the access_mint PDA.
2. **Lines 148-151: Calculate mint account length** with NonTransferable extension.
3. **Lines 153-171: Create account via `create_account` syscall** signed by access_mint PDA.
   - Manager pays rent.
   - Account owned by Token-2022 program.
4. **Lines 174-184: Initialize NonTransferable extension** — Must be done BEFORE InitializeMint (Token-2022 requirement).
5. **Lines 187-199: Initialize Mint** — decimals=0, mint authority = config PDA, no freeze authority.
   - **Key:** Config PDA is the mint authority, meaning only the config PDA can mint tokens. This is used in `mint_access_token_handler`.
   - **No freeze authority:** Tokens cannot be frozen. Combined with NonTransferable, tokens are soulbound and can only be burned by the holder.
6. **Lines 202-203: Store mint pubkey** in reward_token_state.

**Invariants:**
1. One access mint per reward token.
2. Mint authority = config PDA (program-controlled).
3. NonTransferable = soulbound (cannot be transferred between users).
4. No freeze authority.

#### `mint_access_token_handler`

**Purpose:** Mint access tokens to a recipient via Token-2022 CPI.

**Inputs & Assumptions:**
- `authority` (Signer): Must be `config.minter`.
- Program must not be paused.
- Reward token must have a valid access_token_mint.
- Reward token must not be mint-paused.
- `amount` > 0.

**Block-by-Block:**

1. **Line 216: Amount > 0 check.**
2. **Lines 223-243: CPI MintTo** — Config PDA signs as mint authority.
   - **Important:** This instruction DOES NOT check or update supply. Supply tracking is done in `admin_mint` / `mint_with_signature`. This instruction is purely a CPI wrapper.
   - **5 Whys: Why separate?** Likely to keep the Token-2022 CPI isolated from supply logic. But this means `mint_access_token` can be called independently by the minter without going through supply checks IF the minter calls it directly.

**Risk consideration:** The minter can call `mint_access_token` directly without calling `admin_mint` first, bypassing supply tracking. This would mint tokens without incrementing `current_supply`, allowing mints beyond `max_supply`.

#### `burn_access_token_handler`

**Purpose:** Burn access tokens. Called by the token holder.

**Analysis:**
- Holder signs as burn authority (standard Token-2022 burn).
- Program must not be paused.
- Uses `invoke` (not `invoke_signed`) since the holder is the authority.
- **No supply decrement.** Current_supply is never decreased on burn. This means `current_supply` tracks total ever minted, not circulating supply.

---

### 2h: Mint Instructions (`instructions/mint.rs`)

#### `admin_mint_handler`

**Purpose:** Minter role mints access tokens. Validates supply limits and increments current_supply. The actual Token-2022 CPI is expected to be in a separate instruction (`mint_access_token`).

**Inputs:**
- `minter` (Signer): Must be `config.minter`.
- `amount` (u64): Number of access tokens.
- `_is_soulbound` (bool): Used only in event. Prefixed with underscore — not functionally used.

**Block-by-Block:**

1. **Line 100: amount > 0.**
2. **Line 105: Per-token mint pause check.** `!state.is_mint_paused`.
3. **Lines 108-115: Supply check.**
   - `new_supply = current_supply + amount` (checked_add).
   - `new_supply <= max_supply`.
4. **Line 118: Update current_supply.**
5. **Lines 120-126: Emit `Minted` event.**
   - **Note:** `to` field is `minter.key()`, not the actual recipient. The minted event says the minter received tokens, but in practice, `mint_access_token` would mint to a different recipient. **This is a data inconsistency in the event.**

**Invariants:**
1. `current_supply + amount <= max_supply` enforced.
2. Global pause check in account constraints.
3. Per-token mint pause check in handler.

**Assumptions:**
1. `admin_mint` and `mint_access_token` are called atomically (same transaction). If only `admin_mint` is called, supply is incremented but no tokens are minted. If only `mint_access_token` is called, tokens are minted without supply tracking.
2. The `_is_soulbound` parameter is cosmetic — the actual soulbound property is on the Token-2022 mint.

#### `mint_with_signature_handler`

**Purpose:** User-initiated mint. Always mints exactly 1 access token. Requires a valid Ed25519 signature from a whitelisted signer, with replay protection via nonce PDA.

**Inputs:**
- `user` (Signer, mut): The user requesting the mint. Pays for nonce PDA creation.
- `nonce` (u64): Unique nonce for replay protection.
- `_is_soulbound`, `_is_claim_reward` (bool): Event/metadata only.

**Block-by-Block:**

1. **Lines 147-150: Per-token mint pause check.**
2. **Lines 152-159: Supply check** — Always adds 1 (`checked_add(1)`).
3. **Lines 162-176: Ed25519 signature verification**
   - Builds expected message: `user_pubkey(32) | token_id(8 LE) | nonce(8 LE)` = 48 bytes.
   - Calls `verify_ed25519_signature`.
4. **Lines 179-185: Whitelist check** — Verifies extracted signer is in `whitelist_signers`.
5. **Lines 189-192: Mark nonce as used.**
   - **Note:** The nonce PDA is initialized with `init` constraint in the account struct. If the PDA already exists, the entire instruction fails. This is the PRIMARY replay protection — the `used = true` flag is secondary/redundant.
6. **Lines 194-195: Update supply.**
7. **Events emitted:** `UserNonceUsed` and `Minted`.

#### `verify_ed25519_signature` (internal function, lines 227-314)

**Purpose:** Parse the Ed25519 native program instruction from the instructions sysvar and verify the signed message matches expectations.

**Block-by-Block:**

1. **Lines 232-234: Get current instruction index** from sysvar.
2. **Lines 237-239: Require current_ix_index > 0** — The Ed25519 ix must precede this one.
3. **Lines 242-246: Load the instruction at (current_ix_index - 1).**
   - **Key assumption:** Ed25519 instruction is IMMEDIATELY before this instruction. If there are other instructions between, the check would fail on program_id verification.
4. **Lines 249-252: Verify program_id == ed25519_program::ID.**
5. **Lines 256-275: Parse instruction data** — Extract num_signatures, parse Ed25519SignatureOffsets struct.
6. **Lines 277-298: Extract public key and message** from instruction data using offsets.
7. **Lines 301-311: Verify signed message matches expected message.**

**Key observations:**
- Only checks the **first** signature entry in the Ed25519 instruction. If multiple signatures exist, only the first is validated.
- The Ed25519 program itself performs the signature verification. The code here is NOT verifying the signature mathematically — it's extracting the signer's pubkey and the message from the already-verified Ed25519 instruction. Since the Ed25519 program validates the signature, this is correct.
- **No expiry check.** The message format is `user_pubkey | token_id | nonce` with no timestamp. Signatures don't expire — replay is prevented only by the nonce mechanism. The `SignatureExpired` error code exists but is never used.

**Invariants:**
1. Ed25519 instruction must immediately precede the mint instruction.
2. Signed message must exactly match `user_pubkey(32) | token_id(8) | nonce(8)`.
3. Signer must be in the whitelist.
4. Nonce must be unused (PDA init guarantees this).

---

### 2i: Claim Instructions (`instructions/claim.rs`)

#### `claim_reward_handler`

**Purpose:** User claims rewards by burning access tokens and receiving the reward package. The user's access token burn is expected to happen in a separate instruction in the same transaction.

**Inputs:**
- `user` (Signer, mut): The claiming user.
- remaining_accounts: Treasury token accounts, user token accounts, reservation PDAs.

**Account constraints:**
- Program not paused.
- Claim not paused for this reward token.

**Analysis:**
- Delegates to `distribute_rewards` internal function.
- Emits `Claimed` with amount=1.
- **Critical observation:** There is NO verification that the user actually holds or burns access tokens within this instruction. The comment at line 101 says "Task #6 will add burn CPI" — suggesting this is incomplete. A user can call `claim_reward` without holding any access tokens and still receive rewards.

#### `admin_claim_reward_handler`

**Purpose:** Manager claims rewards on behalf of a beneficiary.

**Analysis:**
- Manager-only (account constraint).
- Beneficiary is a non-default Pubkey passed as `AccountInfo`.
- Delegates to same `distribute_rewards`.
- **Same observation:** No access token burn/verification.

#### `distribute_rewards` (internal function, lines 166-314)

**Purpose:** Core reward distribution logic. Iterates through all reward entries for the reward token and transfers each reward from treasury to recipient.

**Inputs:**
- `state` (&mut RewardTokenState): Mutable for NFT index updates.
- `config_key`, `treasury_bump`: For treasury PDA signer.
- `recipient`: Who receives the rewards.
- `treasury_state`, `token_program`: For CPI.
- `remaining_accounts`: Layout depends on reward types.
- `program_id`: For PDA verification.

**Block-by-Block:**

1. **Line 176: Treasury signer seeds.**
2. **Lines 180-311: For each reward entry:**

   **SOL (lines 184-198):**
   - Reads treasury lamports.
   - Requires `treasury_ai.lamports() >= amount`.
   - Direct lamport manipulation: debit treasury, credit recipient.
   - **No reservation decrement for SOL** — consistent with no SOL reservation system.
   - **No checked_sub on lamport debit** — uses direct subtraction via `try_borrow_mut_lamports`. The `require!` check before prevents underflow, but this is not using checked arithmetic on the actual debit.
   - **Important:** Does not check rent-exemption after debit. If enough SOL claims are made, the treasury PDA could drop below rent-exemption, potentially causing the account to be garbage collected.

   **SplToken (lines 199-246):**
   - Consumes 3 remaining accounts: treasury_token_ata, user_token_ata, token_reservation.
   - Transfers `reward.amount` tokens via CPI.
   - Verifies reservation PDA seeds.
   - Decrements `reservation.reserved_amount` by `reward.amount` (checked_sub).
   - Calls `reservation.exit()` to serialize.
   - **No verification** that treasury_token_ata is actually owned by treasury or has the correct mint. The CPI transfer might fail if accounts are wrong, but no explicit pre-check.
   - **No verification** that user_token_ata is owned by recipient or has the correct mint.

   **Nft (lines 248-308):**
   - Uses `nft_current_index` to know which NFTs to distribute next.
   - For each NFT in range `[current_index, current_index + amount)`:
     - Consumes 3 remaining accounts: treasury_nft_ata, user_nft_ata, nft_reservation.
     - Transfers 1 NFT via CPI.
     - Verifies nft_reservation PDA matches the NFT mint from `nft_mints[nft_idx]`.
     - Sets `nft_reservation.is_reserved = false`.
     - Calls `exit()`.
   - **Line 308:** Updates `nft_current_index += nft_count`.
   - **Bounds check (line 255-258):** `nft_idx < reward.nft_mints.len()` — prevents out-of-bounds.

**Key Observation — No claim rate limiting / access token verification:**
- `distribute_rewards` does not check that the user actually has or burns access tokens. It distributes rewards unconditionally.
- The intended flow is: user calls `burn_access_token` + `claim_reward` in the same transaction. But nothing enforces this atomicity. A user can call `claim_reward` without burning tokens.
- **Similarly, `admin_claim_reward` doesn't verify anything about the beneficiary holding access tokens.**

**Key Observation — No claim counting:**
- There's no tracking of how many times a reward has been claimed. For SOL and SplToken rewards, the treasury balance and reservation amounts serve as implicit limits. For NFT rewards, the `nft_current_index` advancing past `nft_mints.len()` prevents over-claiming.
- But a single user could call `claim_reward` repeatedly (as long as treasury has funds and reservations haven't been exhausted), draining the treasury without burning any access tokens.

---

## Phase 3 — Global System Understanding

### 3.1 State & Invariant Reconstruction

#### Global Invariants (Expected)

| ID | Invariant | Where Enforced | Status |
|---|---|---|---|
| I1 | `current_supply <= max_supply` for each RewardTokenState | `admin_mint`, `mint_with_signature` | Enforced at mint time |
| I2 | For SPL rewards: `sum(reservations for mint) <= treasury balance` | `create_reward_token`, `increase_reward_supply` | Enforced at creation/supply-increase, but NOT at claim time (reservation decremented, treasury debited — consistent) |
| I3 | For NFT rewards: each NFT is reserved by at most one reward token | `create_reward_token` NftReservation check | Enforced |
| I4 | Nonce PDA can only be created once per (user, nonce) | `init` constraint on UserNonce | Enforced by Anchor |
| I5 | Only authorized roles can execute role-gated instructions | Account constraints | Enforced |
| I6 | `nft_current_index` advances monotonically and never exceeds `nft_mints.len()` | `distribute_rewards` bounds check | Enforced within a single claim, but no cross-call guard |
| I7 | **Claims should require access token burn** | **NOT ENFORCED** | **Missing** |

#### State Variable Read/Write Map

| State | Writers | Readers |
|---|---|---|
| `config.admin` | `initialize`, `update_roles` | All instructions (via constraint) |
| `config.manager` | `initialize`, `update_roles` | Many instructions (constraint) |
| `config.minter` | `initialize`, `update_roles` | `mint_access_token`, `admin_mint` |
| `config.dev_config` | `initialize`, `update_roles` | `add/remove_whitelist_signer` |
| `config.is_paused` | `pause`, `unpause` | `mint_access_token`, `burn_access_token`, `mint_with_signature`, `claim_reward`, `admin_claim_reward` |
| `config.reward_token_count` | `initialize`, `create_reward_token` | (informational only) |
| `reward_token_state.current_supply` | `admin_mint`, `mint_with_signature` | Same (supply check) |
| `reward_token_state.max_supply` | `create_reward_token`, `increase_reward_supply` | `admin_mint`, `mint_with_signature` |
| `reward_token_state.rewards[].nft_current_index` | `distribute_rewards` (claim) | `distribute_rewards` (claim) |
| `token_reservation.reserved_amount` | `create_reward_token`, `increase_reward_supply`, `distribute_rewards` | `withdraw_unreserved_treasury` |
| `nft_reservation.is_reserved` | `create_reward_token`, `distribute_rewards` | `withdraw_unreserved_nft` |
| `whitelist_signers.signers` | `add/remove_whitelist_signer` | `mint_with_signature` |
| `token_whitelist.tokens` | `whitelist_token`, `remove_token_from_whitelist` | `deposit_to_treasury`, `deposit_nft_to_treasury` |

### 3.2 End-to-End Workflow Reconstruction

#### Workflow 1: Setup & Configuration
```
1. initialize(manager, minter, dev_config)    -> Creates global PDAs
2. add_whitelist_signer(signer_pubkey)         -> Adds Ed25519 signer
3. whitelist_token(mint, SplToken)             -> Enables treasury deposits for SPL
4. whitelist_token(nft_mint, Nft)              -> Enables treasury deposits for NFTs
```

#### Workflow 2: Treasury Funding
```
1. init_token_reservation(mint)                -> Creates reservation PDA for SPL token
2. init_nft_reservation(nft_mint)              -> Creates reservation PDA per NFT
3. deposit_to_treasury(amount)                 -> Anyone deposits SPL tokens
4. deposit_nft_to_treasury()                   -> Anyone deposits NFTs
5. (SOL: Transfer lamports directly to treasury PDA)
```

#### Workflow 3: Reward Token Creation
```
1. create_reward_token(token_id, uri, max_supply, rewards)
   -> Validates treasury has enough, reserves amounts
2. create_access_token_mint()
   -> Creates Token-2022 soulbound mint
```

#### Workflow 4: User Minting (Signature-based)
```
1. Backend signs: Ed25519(user_pubkey | token_id | nonce) with whitelisted key
2. User submits tx:
   - Instruction 0: Ed25519 native program (signature verification)
   - Instruction 1: mint_with_signature(nonce, is_soulbound, is_claim_reward)
   - (Instruction 2: mint_access_token(1) — separate CPI for Token-2022 mint)
```

#### Workflow 5: Admin Minting
```
1. admin_mint(amount, is_soulbound)            -> Validates and increments supply
2. mint_access_token(amount)                   -> CPI mints Token-2022 tokens
```

#### Workflow 6: Claiming
```
Expected flow:
1. burn_access_token(1)                        -> User burns 1 access token
2. claim_reward()                              -> Distributes all reward entries to user

Actual enforcement: Steps are independent. claim_reward does NOT verify burn occurred.
```

#### Workflow 7: Admin Claiming
```
1. admin_claim_reward()                        -> Manager distributes rewards to beneficiary
   (No access token burn required)
```

### 3.3 Trust Boundary Mapping

```
                    +----------------------------------+
                    |          ADMIN (Full Trust)       |
                    |  - Can change all roles           |
                    |  - Can brick system by setting    |
                    |    roles to inaccessible keys     |
                    +-----------------+----------------+
                                      | update_roles
         +----------------------------+----------------------------+
         |                            |                            |
    +----v-----+              +-------v------+             +-------v-------+
    | MANAGER  |              |   MINTER     |             |  DEV_CONFIG   |
    | (High)   |              |  (Medium)    |             |  (Medium)     |
    |          |              |              |             |               |
    | create   |              | admin_mint   |             | manage        |
    | tokens,  |              | mint_access  |             | whitelist     |
    | treasury |              |              |             | signers       |
    | pause    |              +--------------+             +---------------+
    | claim    |
    +----+-----+
         |
    +----v-------------------------------------------------+
    |              UNTRUSTED USERS                          |
    |                                                       |
    |  deposit_to_treasury  (anyone, if whitelisted)        |
    |  deposit_nft          (anyone, if whitelisted)        |
    |  mint_with_signature  (requires valid sig)            |
    |  burn_access_token    (token holder)                  |
    |  claim_reward         (anyone -- MISSING CHECK)       |
    +-------------------------------------------------------+
```

**Critical trust boundary observations:**
1. `claim_reward` has no access control beyond program-not-paused and claim-not-paused. Any signer can drain treasury reserves.
2. `admin_claim_reward` is manager-gated but sends rewards to arbitrary beneficiary without any access token verification.
3. `deposit_to_treasury` and `deposit_nft_to_treasury` have no pause check — deposits work even when program is paused.
4. `mint_access_token` is separately callable from `admin_mint` — trusted minter could mint beyond tracked supply.

### 3.4 Complexity & Fragility Clusters

#### Cluster 1: remaining_accounts Processing (HIGH FRAGILITY)
- `create_reward_token`, `increase_reward_supply`, `claim_reward`, `admin_claim_reward`
- These all parse `remaining_accounts` with a rolling index and type-dependent layouts.
- Manual PDA verification, manual deserialization, manual `exit()` calls.
- Any mismatch in the expected layout silently causes wrong accounts to be used.
- **Fragility:** The contract relies on the client providing accounts in the exact right order. Wrong ordering could lead to:
  - Updating the wrong reservation PDA
  - Transferring tokens to/from wrong accounts

#### Cluster 2: Supply Tracking vs Actual Token Minting (HIGH FRAGILITY)
- `admin_mint` increments `current_supply` but doesn't mint tokens.
- `mint_access_token` mints tokens but doesn't check `current_supply`.
- These MUST be called together but nothing enforces this.
- **Fragility:** Desynchronization between tracked supply and actual Token-2022 supply.

#### Cluster 3: Claim Without Burn Verification (CRITICAL)
- `claim_reward` distributes all rewards without verifying the user burned access tokens.
- The comment at `claim.rs:101` says "Task #6 will add burn CPI" — this is incomplete.
- **Fragility:** Anyone can call `claim_reward` repeatedly to drain the treasury.

#### Cluster 4: SOL Reward Handling (MEDIUM FRAGILITY)
- SOL rewards have no reservation system.
- SOL balance can change between creation and claim time.
- Treasury PDA could go below rent-exemption after SOL claims.
- **Fragility:** SOL rewards depend on manager maintaining sufficient lamport balance manually.

#### Cluster 5: `remove_token_from_whitelist` Reservation Check (MEDIUM FRAGILITY)
- The SPL reservation check is optional (depends on remaining_accounts being provided).
- No check for NFT reservations at all.
- **Fragility:** Manager can remove a whitelisted token that has active reservations.

#### Cluster 6: increase_reward_supply NFT Gap (MEDIUM FRAGILITY)
- NFT supply increase silently skips NFT handling.
- max_supply increases but no new NFT mints are added.
- Claims beyond original supply will fail at NFT distribution.

---

## Summary — Top Findings for Vulnerability Hunting

| Priority | Area | Issue |
|---|---|---|
| **CRITICAL** | `claim_reward` / `admin_claim_reward` | No access token burn verification — anyone can call `claim_reward` to receive rewards without burning tokens |
| **HIGH** | `mint_access_token` vs `admin_mint` | Independently callable — minter can bypass supply tracking by calling `mint_access_token` directly |
| **HIGH** | remaining_accounts processing | Manual account parsing in `create_reward_token`, `increase_reward_supply`, `claim_reward` — fragile, relies on client ordering |
| **MEDIUM** | `remove_token_from_whitelist` | SPL reservation check is optional (remaining_accounts), NFT reservation check is absent |
| **MEDIUM** | `increase_reward_supply` for NFTs | Silently skips NFT provisioning — creates inconsistent state between max_supply and available NFTs |
| **MEDIUM** | SOL rewards | No reservation system — treasury can become insolvent for SOL rewards; potential rent-exemption violation |
