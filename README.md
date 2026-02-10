# Summon Rewards - Solana/Anchor

Solana port of the Summon Rewards system using Anchor framework.

## Prerequisites

- Rust 1.75+
- Solana CLI 2.0+
- Anchor CLI 0.30.1
- Node.js 18+

## Build

```bash
anchor build
```

## Test

```bash
anchor test
```

## Project Structure

```
programs/summon_rewards/    Anchor program source
  src/
    lib.rs                  Program entrypoint
    errors.rs               Custom error codes
    events.rs               Event definitions
    instructions/           Instruction handlers
      initialize.rs         Program initialization
      admin.rs              Admin role management
      treasury.rs           Treasury vault management
      reward_token.rs       Reward token CRUD
      mint.rs               Minting (admin + signature)
      claim.rs              Reward claiming/distribution
      access_token.rs       Soulbound Token-2022 tokens
    state/                  Account state definitions
      config.rs             Global config PDA
      whitelist.rs          Token whitelist
      signers.rs            Whitelist signers
      treasury.rs           Treasury state
      reservation.rs        Token/NFT reservations
      nonce.rs              User nonce tracking
      reward_token.rs       Reward token state
  tests/                    Rust integration tests
tests/                      TypeScript integration tests
migrations/                 Deploy scripts
```
