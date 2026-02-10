use anchor_lang::prelude::*;

/// Custom error codes for the Summon Rewards program.
/// Maps from error declarations across Rewards.sol, Treasury.sol, and RewardsState.sol.
#[error_code]
pub enum SummonRewardsError {
    // ─── Access Control ──────────────────────────────────────────────
    #[msg("Unauthorized: signer does not have the required role")]
    Unauthorized,

    // ─── Pause ───────────────────────────────────────────────────────
    #[msg("Program is paused")]
    ProgramPaused,

    #[msg("Minting is paused for this token")]
    MintPaused,

    #[msg("Claiming is paused for this token")]
    ClaimRewardPaused,

    // ─── Input Validation ────────────────────────────────────────────
    #[msg("Address must not be default/zero")]
    AddressIsZero,

    #[msg("Invalid token ID")]
    InvalidTokenId,

    #[msg("Invalid amount: must be greater than zero")]
    InvalidAmount,

    #[msg("Invalid input parameters")]
    InvalidInput,

    #[msg("Invalid length: arrays must match")]
    InvalidLength,

    // ─── Token Existence ─────────────────────────────────────────────
    #[msg("Token does not exist")]
    TokenNotExist,

    #[msg("Duplicate token ID")]
    DupTokenId,

    // ─── Whitelist ───────────────────────────────────────────────────
    #[msg("Token is not whitelisted")]
    TokenNotWhitelisted,

    #[msg("Token is already whitelisted")]
    TokenAlreadyWhitelisted,

    #[msg("Cannot remove: token has active reservations")]
    TokenHasReserves,

    // ─── Supply ──────────────────────────────────────────────────────
    #[msg("Minting would exceed max supply")]
    ExceedMaxSupply,

    #[msg("Cannot reduce supply")]
    CannotReduceSupply,

    // ─── Treasury / Balance ──────────────────────────────────────────
    #[msg("Insufficient balance")]
    InsufficientBalance,

    #[msg("Insufficient treasury balance for reservation")]
    InsufficientTreasuryBalance,

    #[msg("Transfer failed")]
    TransferFailed,

    // ─── Signature Verification ──────────────────────────────────────
    #[msg("Invalid Ed25519 signature")]
    InvalidSignature,

    #[msg("Signature has expired")]
    SignatureExpired,

    #[msg("Nonce has already been used")]
    NonceAlreadyUsed,

    #[msg("Signer is not in the whitelist")]
    SignerNotWhitelisted,

    #[msg("Signer is already in the whitelist")]
    SignerAlreadyWhitelisted,

    #[msg("Missing Ed25519 instruction")]
    MissingEd25519Instruction,

    // ─── NFT Specific ────────────────────────────────────────────────
    #[msg("NFT is already reserved")]
    NftAlreadyReserved,

    #[msg("NFT is not owned by treasury")]
    NftNotInTreasury,

    // ─── Arithmetic ──────────────────────────────────────────────────
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,
}
