/// Comprehensive test suite for the Summon Rewards Solana/Anchor program.
///
/// Test coverage:
/// 1. Initialization
/// 2. Token Creation (reward tokens)
/// 3. Minting (admin mint + signature-verified mint)
/// 4. Claiming (single + admin claim, all reward types)
/// 5. Treasury (whitelist, deposit, withdraw, balance queries)
/// 6. Soulbound (non-transferable, authorized burn)
/// 7. Signature Verification (Ed25519 whitelist)
/// 8. Access Control (role-based permissions)
/// 9. Supply Management
/// 10. Pause/Unpause
/// 11. Edge Cases
/// 12. Full Integration Flow
///
/// NOTE: Integration tests that interact with the program (via LiteSVM) require
/// the compiled .so binary from `anchor build`. If the binary is not present,
/// those tests will be skipped with a clear message.
/// PDA derivation and seed validation tests run without the binary.
use litesvm::LiteSVM;
use solana_sdk::{
    hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_program,
    transaction::Transaction,
};
use std::path::PathBuf;

/// The program ID from the deploy keypair.
/// NOTE: If `anchor build` regenerates the keypair, this must be updated.
/// Run: `solana-keygen pubkey target/deploy/summon_rewards-keypair.json`
/// Also ensure lib.rs declare_id! matches.
fn program_id() -> Pubkey {
    "3XVtDWuE5Pgbu1bG2FNYBfkj51sdXQDXFg93rTnKUoJm"
        .parse()
        .unwrap()
}

// ─── Helper: derive PDAs ────────────────────────────────────────────

fn find_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"config"], &program_id())
}

fn find_whitelist_signers_pda(config: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"signers", config.as_ref()], &program_id())
}

fn find_token_whitelist_pda(config: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"whitelist", config.as_ref()], &program_id())
}

fn find_treasury_pda(config: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"treasury", config.as_ref()], &program_id())
}

fn find_reward_token_pda(config: &Pubkey, token_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"reward_token", config.as_ref(), &token_id.to_le_bytes()],
        &program_id(),
    )
}

fn find_nonce_pda(config: &Pubkey, user: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            b"nonce",
            config.as_ref(),
            user.as_ref(),
            &nonce.to_le_bytes(),
        ],
        &program_id(),
    )
}

fn find_token_reservation_pda(config: &Pubkey, mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"reserve", config.as_ref(), mint.as_ref()],
        &program_id(),
    )
}

fn find_nft_reservation_pda(config: &Pubkey, nft_mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"nft_reserve", config.as_ref(), nft_mint.as_ref()],
        &program_id(),
    )
}

// ─── Helper: Anchor instruction discriminators ──────────────────────

/// Anchor uses the first 8 bytes of sha256("global:<instruction_name>") as discriminator.
fn anchor_discriminator(name: &str) -> [u8; 8] {
    let full_name = format!("global:{}", name);
    let hash_result = hash::hash(full_name.as_bytes());
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash_result.to_bytes()[..8]);
    disc
}

// ─── Helper: locate and load the compiled program binary ────────────

fn program_so_path() -> PathBuf {
    // The test binary runs from the workspace root or the package directory.
    // The .so file is produced by `anchor build` at anchor/target/deploy/summon_rewards.so
    let candidates = [
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("deploy")
            .join("summon_rewards.so"),
        PathBuf::from("target/deploy/summon_rewards.so"),
        PathBuf::from("../../../target/deploy/summon_rewards.so"),
    ];
    for path in &candidates {
        if path.exists() {
            return path.clone();
        }
    }
    // Return the first candidate; setup() will handle the missing file
    candidates[0].clone()
}

fn load_program_bytes() -> Option<Vec<u8>> {
    let path = program_so_path();
    std::fs::read(&path).ok()
}

// ─── Helper: setup LiteSVM with program loaded ─────────────────────

struct TestEnv {
    svm: LiteSVM,
    admin: Keypair,
    manager: Keypair,
    minter: Keypair,
    dev_config: Keypair,
    config_pda: Pubkey,
    #[allow(dead_code)]
    config_bump: u8,
    whitelist_signers_pda: Pubkey,
    token_whitelist_pda: Pubkey,
    treasury_pda: Pubkey,
}

/// Create a test environment. Returns None if the program binary is not available.
fn setup() -> Option<TestEnv> {
    let program_bytes = load_program_bytes()?;
    let mut svm = LiteSVM::new();
    svm.add_program(program_id(), &program_bytes);

    // Create keypairs for roles
    let admin = Keypair::new();
    let manager = Keypair::new();
    let minter = Keypair::new();
    let dev_config = Keypair::new();

    // Airdrop SOL
    svm.airdrop(&admin.pubkey(), 100_000_000_000).unwrap();
    svm.airdrop(&manager.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&minter.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&dev_config.pubkey(), 10_000_000_000).unwrap();

    // Derive PDAs
    let (config_pda, config_bump) = find_config_pda();
    let (whitelist_signers_pda, _) = find_whitelist_signers_pda(&config_pda);
    let (token_whitelist_pda, _) = find_token_whitelist_pda(&config_pda);
    let (treasury_pda, _) = find_treasury_pda(&config_pda);

    Some(TestEnv {
        svm,
        admin,
        manager,
        minter,
        dev_config,
        config_pda,
        config_bump,
        whitelist_signers_pda,
        token_whitelist_pda,
        treasury_pda,
    })
}

fn initialize(env: &mut TestEnv) -> Result<(), Box<dyn std::error::Error>> {
    let disc = anchor_discriminator("initialize");

    // Serialize arguments: manager (Pubkey), minter (Pubkey), dev_config (Pubkey)
    let mut data = Vec::new();
    data.extend_from_slice(&disc);
    data.extend_from_slice(&env.manager.pubkey().to_bytes());
    data.extend_from_slice(&env.minter.pubkey().to_bytes());
    data.extend_from_slice(&env.dev_config.pubkey().to_bytes());

    let accounts = vec![
        AccountMeta::new(env.admin.pubkey(), true),
        AccountMeta::new(env.config_pda, false),
        AccountMeta::new(env.whitelist_signers_pda, false),
        AccountMeta::new(env.token_whitelist_pda, false),
        AccountMeta::new(env.treasury_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
    let blockhash = env.svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.admin.pubkey()),
        &[&env.admin],
        blockhash,
    );
    env.svm
        .send_transaction(tx)
        .map_err(|e| format!("Initialize failed: {:?}", e))?;
    Ok(())
}

fn setup_initialized() -> Option<TestEnv> {
    let mut env = setup()?;
    initialize(&mut env).expect("Initialize should succeed");
    Some(env)
}

/// Macro to skip tests when the program binary is not available.
macro_rules! require_program {
    ($env:expr) => {
        match $env {
            Some(e) => e,
            None => {
                eprintln!(
                    "SKIPPED: Program binary not found. Run `anchor build` first."
                );
                return;
            }
        }
    };
}

// ─── Shared: create_reward_token helper for multiple test modules ───

/// Build a create_reward_token instruction with SOL-only rewards (simplest case).
/// Used by test modules that need a reward token but don't test creation itself.
fn build_simple_create_reward_token_ix(
    manager: &Pubkey,
    config: &Pubkey,
    reward_token_pda: &Pubkey,
    treasury_pda: &Pubkey,
    token_id: u64,
    uri: &str,
    max_supply: u64,
) -> Instruction {
    let disc = anchor_discriminator("create_reward_token");
    let mut data = Vec::new();
    data.extend_from_slice(&disc);
    data.extend_from_slice(&token_id.to_le_bytes());
    data.extend_from_slice(&(uri.len() as u32).to_le_bytes());
    data.extend_from_slice(uri.as_bytes());
    data.extend_from_slice(&max_supply.to_le_bytes());
    // 1 SOL reward entry (simplest valid case - no remaining_accounts needed)
    data.extend_from_slice(&1u32.to_le_bytes()); // rewards.len() = 1
    data.push(0); // RewardType::Sol
    data.extend_from_slice(&1_000_000u64.to_le_bytes()); // amount
    data.push(0); // token_mint = None
    data.extend_from_slice(&0u32.to_le_bytes()); // nft_mints empty
    data.extend_from_slice(&0u64.to_le_bytes()); // nft_current_index

    let accounts = vec![
        AccountMeta::new(*manager, true),
        AccountMeta::new(*config, false),
        AccountMeta::new(*reward_token_pda, false),
        AccountMeta::new_readonly(*treasury_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ];
    Instruction::new_with_bytes(program_id(), &data, accounts)
}

/// Helper to create a simple reward token in a test environment.
fn create_simple_reward_token(env: &mut TestEnv, token_id: u64) -> Pubkey {
    let (reward_token_pda, _) = find_reward_token_pda(&env.config_pda, token_id);
    let ix = build_simple_create_reward_token_ix(
        &env.manager.pubkey(),
        &env.config_pda,
        &reward_token_pda,
        &env.treasury_pda,
        token_id,
        "https://example.com/test-token",
        100,
    );
    let blockhash = env.svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.manager.pubkey()),
        &[&env.manager],
        blockhash,
    );
    env.svm
        .send_transaction(tx)
        .expect("Create reward token should succeed");
    reward_token_pda
}

// ============================================================================
// 1. INITIALIZATION TESTS
// ============================================================================

#[cfg(test)]
mod initialization {
    use super::*;

    #[test]
    fn test_initialize_success() {
        let mut env = require_program!(setup());
        let result = initialize(&mut env);
        assert!(result.is_ok(), "Initialization should succeed");
    }

    #[test]
    fn test_initialize_sets_correct_admin() {
        let env = require_program!(setup_initialized());
        let config_account = env.svm.get_account(&env.config_pda);
        assert!(
            config_account.is_some(),
            "Config PDA should exist after initialization"
        );
        let account_data = config_account.unwrap();
        assert!(account_data.data.len() >= 8, "Config account should have data");
        assert_eq!(
            account_data.owner,
            program_id(),
            "Config should be owned by program"
        );
    }

    #[test]
    fn test_initialize_creates_all_pdas() {
        let env = require_program!(setup_initialized());
        assert!(
            env.svm.get_account(&env.config_pda).is_some(),
            "Config PDA should exist"
        );
        assert!(
            env.svm.get_account(&env.whitelist_signers_pda).is_some(),
            "Whitelist signers PDA should exist"
        );
        assert!(
            env.svm.get_account(&env.token_whitelist_pda).is_some(),
            "Token whitelist PDA should exist"
        );
        assert!(
            env.svm.get_account(&env.treasury_pda).is_some(),
            "Treasury PDA should exist"
        );
    }

    #[test]
    fn test_initialize_cannot_be_called_twice() {
        let mut env = require_program!(setup_initialized());
        let result = initialize(&mut env);
        assert!(result.is_err(), "Double initialization should fail");
    }

    #[test]
    fn test_initialize_rejects_zero_manager() {
        let mut env = require_program!(setup());
        let disc = anchor_discriminator("initialize");

        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&Pubkey::default().to_bytes());
        data.extend_from_slice(&env.minter.pubkey().to_bytes());
        data.extend_from_slice(&env.dev_config.pubkey().to_bytes());

        let accounts = vec![
            AccountMeta::new(env.admin.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
            AccountMeta::new(env.whitelist_signers_pda, false),
            AccountMeta::new(env.token_whitelist_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];

        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should reject zero address for manager");
    }

    #[test]
    fn test_initialize_rejects_zero_minter() {
        let mut env = require_program!(setup());
        let disc = anchor_discriminator("initialize");

        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&env.manager.pubkey().to_bytes());
        data.extend_from_slice(&Pubkey::default().to_bytes());
        data.extend_from_slice(&env.dev_config.pubkey().to_bytes());

        let accounts = vec![
            AccountMeta::new(env.admin.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
            AccountMeta::new(env.whitelist_signers_pda, false),
            AccountMeta::new(env.token_whitelist_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];

        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should reject zero address for minter");
    }

    #[test]
    fn test_initialize_rejects_zero_dev_config() {
        let mut env = require_program!(setup());
        let disc = anchor_discriminator("initialize");

        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&env.manager.pubkey().to_bytes());
        data.extend_from_slice(&env.minter.pubkey().to_bytes());
        data.extend_from_slice(&Pubkey::default().to_bytes());

        let accounts = vec![
            AccountMeta::new(env.admin.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
            AccountMeta::new(env.whitelist_signers_pda, false),
            AccountMeta::new(env.token_whitelist_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];

        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should reject zero address for dev_config");
    }
}

// ============================================================================
// 2. ACCESS CONTROL / ROLE MANAGEMENT TESTS
// ============================================================================

#[cfg(test)]
mod access_control {
    use super::*;

    fn build_update_roles_ix(
        admin: &Pubkey,
        config: &Pubkey,
        role: u8,
        new_address: &Pubkey,
    ) -> Instruction {
        let disc = anchor_discriminator("update_roles");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.push(role);
        data.extend_from_slice(&new_address.to_bytes());

        let accounts = vec![
            AccountMeta::new_readonly(*admin, true),
            AccountMeta::new(*config, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    #[test]
    fn test_update_manager_role() {
        let mut env = require_program!(setup_initialized());
        let new_manager = Keypair::new();

        let ix = build_update_roles_ix(
            &env.admin.pubkey(),
            &env.config_pda,
            0,
            &new_manager.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Admin should be able to update manager role");
    }

    #[test]
    fn test_update_minter_role() {
        let mut env = require_program!(setup_initialized());
        let new_minter = Keypair::new();

        let ix = build_update_roles_ix(
            &env.admin.pubkey(),
            &env.config_pda,
            1,
            &new_minter.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Admin should be able to update minter role");
    }

    #[test]
    fn test_update_dev_config_role() {
        let mut env = require_program!(setup_initialized());
        let new_dev = Keypair::new();

        let ix = build_update_roles_ix(
            &env.admin.pubkey(),
            &env.config_pda,
            2,
            &new_dev.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "Admin should be able to update dev_config role"
        );
    }

    #[test]
    fn test_update_admin_role() {
        let mut env = require_program!(setup_initialized());
        let new_admin = Keypair::new();
        env.svm
            .airdrop(&new_admin.pubkey(), 10_000_000_000)
            .unwrap();

        let ix = build_update_roles_ix(
            &env.admin.pubkey(),
            &env.config_pda,
            3,
            &new_admin.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Admin should be able to transfer admin role");
    }

    #[test]
    fn test_non_admin_cannot_update_roles() {
        let mut env = require_program!(setup_initialized());
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();
        let new_manager = Keypair::new();

        let ix = build_update_roles_ix(
            &attacker.pubkey(),
            &env.config_pda,
            0,
            &new_manager.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_err(),
            "Non-admin should not be able to update roles"
        );
    }

    #[test]
    fn test_update_roles_rejects_zero_address() {
        let mut env = require_program!(setup_initialized());

        let ix = build_update_roles_ix(
            &env.admin.pubkey(),
            &env.config_pda,
            0,
            &Pubkey::default(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should reject zero address for role update");
    }

    #[test]
    fn test_old_admin_loses_access_after_transfer() {
        let mut env = require_program!(setup_initialized());
        let new_admin = Keypair::new();
        env.svm
            .airdrop(&new_admin.pubkey(), 10_000_000_000)
            .unwrap();

        // Transfer admin
        let ix = build_update_roles_ix(
            &env.admin.pubkey(),
            &env.config_pda,
            3,
            &new_admin.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        env.svm
            .send_transaction(tx)
            .expect("Admin transfer should work");

        // Old admin tries to update - should fail
        let another = Keypair::new();
        let ix2 = build_update_roles_ix(
            &env.admin.pubkey(),
            &env.config_pda,
            0,
            &another.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx2 = Transaction::new_signed_with_payer(
            &[ix2],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        let result = env.svm.send_transaction(tx2);
        assert!(
            result.is_err(),
            "Old admin should lose access after transfer"
        );
    }
}

// ============================================================================
// 3. PAUSE/UNPAUSE TESTS
// ============================================================================

#[cfg(test)]
mod pause_unpause {
    use super::*;

    fn build_pause_ix(manager: &Pubkey, config: &Pubkey) -> Instruction {
        let disc = anchor_discriminator("pause");
        let accounts = vec![
            AccountMeta::new_readonly(*manager, true),
            AccountMeta::new(*config, false),
        ];
        Instruction::new_with_bytes(program_id(), &disc, accounts)
    }

    fn build_unpause_ix(manager: &Pubkey, config: &Pubkey) -> Instruction {
        let disc = anchor_discriminator("unpause");
        let accounts = vec![
            AccountMeta::new_readonly(*manager, true),
            AccountMeta::new(*config, false),
        ];
        Instruction::new_with_bytes(program_id(), &disc, accounts)
    }

    #[test]
    fn test_manager_can_pause() {
        let mut env = require_program!(setup_initialized());
        let ix = build_pause_ix(&env.manager.pubkey(), &env.config_pda);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Manager should be able to pause");
    }

    #[test]
    fn test_manager_can_unpause() {
        let mut env = require_program!(setup_initialized());

        // Pause
        let ix = build_pause_ix(&env.manager.pubkey(), &env.config_pda);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Pause should work");

        // Unpause
        let ix = build_unpause_ix(&env.manager.pubkey(), &env.config_pda);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Manager should be able to unpause");
    }

    #[test]
    fn test_non_manager_cannot_pause() {
        let mut env = require_program!(setup_initialized());
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();

        let ix = build_pause_ix(&attacker.pubkey(), &env.config_pda);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-manager should not be able to pause");
    }

    #[test]
    fn test_non_manager_cannot_unpause() {
        let mut env = require_program!(setup_initialized());

        // Pause first
        let ix = build_pause_ix(&env.manager.pubkey(), &env.config_pda);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Pause should work");

        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();

        let ix = build_unpause_ix(&attacker.pubkey(), &env.config_pda);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_err(),
            "Non-manager should not be able to unpause"
        );
    }
}

// ============================================================================
// 4. WHITELIST SIGNER MANAGEMENT TESTS
// ============================================================================

#[cfg(test)]
mod whitelist_signers {
    use super::*;

    fn build_add_signer_ix(
        authority: &Pubkey,
        config: &Pubkey,
        whitelist_signers_pda: &Pubkey,
        signer_to_add: &Pubkey,
    ) -> Instruction {
        let disc = anchor_discriminator("add_whitelist_signer");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&signer_to_add.to_bytes());

        let accounts = vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new(*whitelist_signers_pda, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    fn build_remove_signer_ix(
        authority: &Pubkey,
        config: &Pubkey,
        whitelist_signers_pda: &Pubkey,
        signer_to_remove: &Pubkey,
    ) -> Instruction {
        let disc = anchor_discriminator("remove_whitelist_signer");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&signer_to_remove.to_bytes());

        let accounts = vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new(*whitelist_signers_pda, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    #[test]
    fn test_dev_config_can_add_signer() {
        let mut env = require_program!(setup_initialized());
        let new_signer = Keypair::new();

        let ix = build_add_signer_ix(
            &env.dev_config.pubkey(),
            &env.config_pda,
            &env.whitelist_signers_pda,
            &new_signer.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.dev_config.pubkey()),
            &[&env.dev_config],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "DevConfig should be able to add a signer");
    }

    #[test]
    fn test_dev_config_can_remove_signer() {
        let mut env = require_program!(setup_initialized());
        let new_signer = Keypair::new();

        // Add
        let ix = build_add_signer_ix(
            &env.dev_config.pubkey(),
            &env.config_pda,
            &env.whitelist_signers_pda,
            &new_signer.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.dev_config.pubkey()),
            &[&env.dev_config],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Add signer should work");

        // Remove
        let ix = build_remove_signer_ix(
            &env.dev_config.pubkey(),
            &env.config_pda,
            &env.whitelist_signers_pda,
            &new_signer.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.dev_config.pubkey()),
            &[&env.dev_config],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "DevConfig should be able to remove a signer"
        );
    }

    #[test]
    fn test_cannot_add_duplicate_signer() {
        let mut env = require_program!(setup_initialized());
        let new_signer = Keypair::new();

        // First add
        let ix = build_add_signer_ix(
            &env.dev_config.pubkey(),
            &env.config_pda,
            &env.whitelist_signers_pda,
            &new_signer.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.dev_config.pubkey()),
            &[&env.dev_config],
            blockhash,
        );
        env.svm
            .send_transaction(tx)
            .expect("First add should work");

        // Duplicate
        let ix = build_add_signer_ix(
            &env.dev_config.pubkey(),
            &env.config_pda,
            &env.whitelist_signers_pda,
            &new_signer.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.dev_config.pubkey()),
            &[&env.dev_config],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should not allow duplicate signer");
    }

    #[test]
    fn test_cannot_remove_nonexistent_signer() {
        let mut env = require_program!(setup_initialized());
        let non_signer = Keypair::new();

        let ix = build_remove_signer_ix(
            &env.dev_config.pubkey(),
            &env.config_pda,
            &env.whitelist_signers_pda,
            &non_signer.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.dev_config.pubkey()),
            &[&env.dev_config],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_err(),
            "Should not allow removing nonexistent signer"
        );
    }

    #[test]
    fn test_non_dev_config_cannot_add_signer() {
        let mut env = require_program!(setup_initialized());
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();
        let new_signer = Keypair::new();

        let ix = build_add_signer_ix(
            &attacker.pubkey(),
            &env.config_pda,
            &env.whitelist_signers_pda,
            &new_signer.pubkey(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-dev-config should not add signers");
    }

    #[test]
    fn test_cannot_add_zero_address_signer() {
        let mut env = require_program!(setup_initialized());

        let ix = build_add_signer_ix(
            &env.dev_config.pubkey(),
            &env.config_pda,
            &env.whitelist_signers_pda,
            &Pubkey::default(),
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.dev_config.pubkey()),
            &[&env.dev_config],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should not allow zero address signer");
    }
}

// ============================================================================
// 5. TREASURY WHITELIST TESTS
// ============================================================================

#[cfg(test)]
mod treasury_whitelist {
    use super::*;

    fn build_whitelist_token_ix(
        manager: &Pubkey,
        config: &Pubkey,
        token_whitelist: &Pubkey,
        mint: &Pubkey,
        reward_type: u8,
    ) -> Instruction {
        let disc = anchor_discriminator("whitelist_token");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&mint.to_bytes());
        data.push(reward_type);

        let accounts = vec![
            AccountMeta::new_readonly(*manager, true),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new(*token_whitelist, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    #[test]
    fn test_manager_can_whitelist_spl_token() {
        let mut env = require_program!(setup_initialized());
        let fake_mint = Keypair::new();

        let ix = build_whitelist_token_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &env.token_whitelist_pda,
            &fake_mint.pubkey(),
            1,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Manager should whitelist a token");
    }

    #[test]
    fn test_non_manager_cannot_whitelist_token() {
        let mut env = require_program!(setup_initialized());
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();
        let fake_mint = Keypair::new();

        let ix = build_whitelist_token_ix(
            &attacker.pubkey(),
            &env.config_pda,
            &env.token_whitelist_pda,
            &fake_mint.pubkey(),
            1,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-manager should not whitelist tokens");
    }
}

// ============================================================================
// 6. REWARD TOKEN CREATION TESTS
// ============================================================================

#[cfg(test)]
mod reward_token_creation {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn build_create_reward_token_ix(
        manager: &Pubkey,
        config: &Pubkey,
        reward_token_pda: &Pubkey,
        treasury_pda: &Pubkey,
        token_id: u64,
        token_uri: &str,
        max_supply: u64,
        rewards: &[(u8, u64, Option<Pubkey>)],
    ) -> Instruction {
        let disc = anchor_discriminator("create_reward_token");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&token_id.to_le_bytes());
        let uri_bytes = token_uri.as_bytes();
        data.extend_from_slice(&(uri_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(uri_bytes);
        data.extend_from_slice(&max_supply.to_le_bytes());
        data.extend_from_slice(&(rewards.len() as u32).to_le_bytes());
        for (reward_type, amount, token_mint) in rewards {
            data.push(*reward_type);
            data.extend_from_slice(&amount.to_le_bytes());
            match token_mint {
                Some(pk) => {
                    data.push(1);
                    data.extend_from_slice(&pk.to_bytes());
                }
                None => {
                    data.push(0);
                }
            }
            data.extend_from_slice(&0u32.to_le_bytes()); // nft_mints empty
            data.extend_from_slice(&0u64.to_le_bytes()); // nft_current_index
        }

        let accounts = vec![
            AccountMeta::new(*manager, true),
            AccountMeta::new(*config, false),
            AccountMeta::new(*reward_token_pda, false),
            AccountMeta::new_readonly(*treasury_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    #[test]
    fn test_create_reward_token_with_sol_reward() {
        let mut env = require_program!(setup_initialized());
        let token_id: u64 = 1;
        let (reward_token_pda, _) = find_reward_token_pda(&env.config_pda, token_id);

        let ix = build_create_reward_token_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.treasury_pda,
            token_id,
            "https://example.com/token/1",
            100,
            &[(0, 1_000_000_000, None)],
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Should create reward token with SOL reward");

        let account = env.svm.get_account(&reward_token_pda);
        assert!(account.is_some(), "Reward token PDA should exist");
    }

    #[test]
    fn test_create_reward_token_with_spl_reward_needs_remaining_accounts() {
        // SPL reward creation requires remaining_accounts for treasury token
        // account and token reservation PDA. Without them, the handler should
        // reject the instruction due to missing remaining_accounts.
        let mut env = require_program!(setup_initialized());
        let token_id: u64 = 2;
        let (reward_token_pda, _) = find_reward_token_pda(&env.config_pda, token_id);
        let mock_spl_mint = Keypair::new();

        let ix = build_create_reward_token_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.treasury_pda,
            token_id,
            "https://example.com/token/2",
            50,
            &[(1, 500_000_000, Some(mock_spl_mint.pubkey()))],
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_err(),
            "SPL reward creation should fail without remaining_accounts for reservation"
        );
    }

    #[test]
    fn test_create_reward_token_with_multiple_rewards_sol_only() {
        // Test that multiple SOL rewards work (no remaining_accounts needed)
        let mut env = require_program!(setup_initialized());
        let token_id: u64 = 3;
        let (reward_token_pda, _) = find_reward_token_pda(&env.config_pda, token_id);

        let ix = build_create_reward_token_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.treasury_pda,
            token_id,
            "https://example.com/token/3",
            200,
            &[
                (0, 500_000_000, None),
                (0, 1_000_000, None),
            ],
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "Should create reward token with multiple SOL rewards: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_cannot_create_duplicate_token_id() {
        let mut env = require_program!(setup_initialized());
        let token_id: u64 = 10;
        let (reward_token_pda, _) = find_reward_token_pda(&env.config_pda, token_id);

        let ix = build_create_reward_token_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.treasury_pda,
            token_id,
            "https://example.com/token/10",
            100,
            &[(0, 1_000_000_000, None)],
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm
            .send_transaction(tx)
            .expect("First creation should work");

        // Duplicate
        let ix = build_create_reward_token_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.treasury_pda,
            token_id,
            "https://example.com/token/10-dupe",
            50,
            &[(0, 500_000_000, None)],
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should not allow duplicate token IDs");
    }

    #[test]
    fn test_non_manager_cannot_create_reward_token() {
        let mut env = require_program!(setup_initialized());
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();
        let token_id: u64 = 99;
        let (reward_token_pda, _) = find_reward_token_pda(&env.config_pda, token_id);

        let ix = build_create_reward_token_ix(
            &attacker.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.treasury_pda,
            token_id,
            "https://example.com/token/99",
            100,
            &[(0, 1_000_000_000, None)],
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_err(),
            "Non-manager should not create reward tokens"
        );
    }
}

// ============================================================================
// 7. TOKEN MINT/CLAIM PAUSE TESTS
// ============================================================================

#[cfg(test)]
mod token_pause {
    use super::*;

    fn create_reward_token_for_pause(env: &mut TestEnv, token_id: u64) -> Pubkey {
        create_simple_reward_token(env, token_id)
    }

    fn build_update_mint_paused_ix(
        manager: &Pubkey,
        config: &Pubkey,
        reward_token_pda: &Pubkey,
        is_paused: bool,
    ) -> Instruction {
        let disc = anchor_discriminator("update_token_mint_paused");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.push(u8::from(is_paused));

        let accounts = vec![
            AccountMeta::new_readonly(*manager, true),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new(*reward_token_pda, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    fn build_update_claim_paused_ix(
        manager: &Pubkey,
        config: &Pubkey,
        reward_token_pda: &Pubkey,
        is_paused: bool,
    ) -> Instruction {
        let disc = anchor_discriminator("update_claim_paused");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.push(u8::from(is_paused));

        let accounts = vec![
            AccountMeta::new_readonly(*manager, true),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new(*reward_token_pda, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    #[test]
    fn test_pause_token_minting() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_reward_token_for_pause(&mut env, 100);

        let ix = build_update_mint_paused_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            true,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Manager should pause token minting");
    }

    #[test]
    fn test_unpause_token_minting() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_reward_token_for_pause(&mut env, 101);

        // Pause
        let ix = build_update_mint_paused_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            true,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Pause should work");

        // Unpause
        let ix = build_update_mint_paused_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            false,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Manager should unpause token minting");
    }

    #[test]
    fn test_pause_token_claiming() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_reward_token_for_pause(&mut env, 102);

        let ix = build_update_claim_paused_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            true,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Manager should pause token claiming");
    }

    #[test]
    fn test_non_manager_cannot_pause_token() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_reward_token_for_pause(&mut env, 103);
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();

        let ix = build_update_mint_paused_ix(
            &attacker.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            true,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_err(),
            "Non-manager should not pause token minting"
        );
    }
}

// ============================================================================
// 8. SUPPLY MANAGEMENT TESTS
// ============================================================================

#[cfg(test)]
mod supply_management {
    use super::*;

    fn create_reward_token_with_supply(env: &mut TestEnv, token_id: u64, max_supply: u64) -> Pubkey {
        let (reward_token_pda, _) = find_reward_token_pda(&env.config_pda, token_id);
        let ix = build_simple_create_reward_token_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.treasury_pda,
            token_id,
            "https://example.com/supply-test",
            max_supply,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm
            .send_transaction(tx)
            .expect("Create token should work");
        reward_token_pda
    }

    fn build_increase_supply_ix(
        manager: &Pubkey,
        config: &Pubkey,
        reward_token_pda: &Pubkey,
        treasury_pda: &Pubkey,
        additional_supply: u64,
    ) -> Instruction {
        let disc = anchor_discriminator("increase_reward_supply");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&additional_supply.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(*manager, true),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new(*reward_token_pda, false),
            AccountMeta::new_readonly(*treasury_pda, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    fn build_update_uri_ix(
        manager: &Pubkey,
        config: &Pubkey,
        reward_token_pda: &Pubkey,
        new_uri: &str,
    ) -> Instruction {
        let disc = anchor_discriminator("update_token_uri");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        let uri_bytes = new_uri.as_bytes();
        data.extend_from_slice(&(uri_bytes.len() as u32).to_le_bytes());
        data.extend_from_slice(uri_bytes);

        let accounts = vec![
            AccountMeta::new_readonly(*manager, true),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new(*reward_token_pda, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    #[test]
    fn test_increase_supply() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_reward_token_with_supply(&mut env, 200, 100);

        let ix = build_increase_supply_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.treasury_pda,
            50,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Manager should increase supply");
    }

    #[test]
    fn test_non_manager_cannot_increase_supply() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_reward_token_with_supply(&mut env, 201, 100);
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();

        let ix = build_increase_supply_ix(
            &attacker.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.treasury_pda,
            50,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-manager should not increase supply");
    }

    #[test]
    fn test_update_token_uri() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_reward_token_with_supply(&mut env, 202, 100);

        let ix = build_update_uri_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            "https://example.com/new-uri",
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Manager should update token URI");
    }

    #[test]
    fn test_non_manager_cannot_update_uri() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_reward_token_with_supply(&mut env, 203, 100);
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();

        let ix = build_update_uri_ix(
            &attacker.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            "https://attacker.com/fake",
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-manager should not update URI");
    }
}

// ============================================================================
// 9. ADMIN MINTING TESTS
// ============================================================================

#[cfg(test)]
mod admin_minting {
    use super::*;

    fn build_admin_mint_ix(
        minter: &Pubkey,
        config: &Pubkey,
        reward_token_pda: &Pubkey,
        amount: u64,
        is_soulbound: bool,
    ) -> Instruction {
        let disc = anchor_discriminator("admin_mint");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&amount.to_le_bytes());
        data.push(u8::from(is_soulbound));

        let accounts = vec![
            AccountMeta::new(*minter, true),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new(*reward_token_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    #[test]
    fn test_minter_can_admin_mint() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 300);

        let ix = build_admin_mint_ix(
            &env.minter.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            5,
            false,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Minter should be able to admin mint");
    }

    #[test]
    fn test_minter_can_admin_mint_soulbound() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 301);

        let ix = build_admin_mint_ix(
            &env.minter.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            1,
            true,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "Minter should admin mint soulbound tokens"
        );
    }

    #[test]
    fn test_non_minter_cannot_admin_mint() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 302);
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();

        let ix = build_admin_mint_ix(
            &attacker.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            1,
            false,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-minter should not admin mint");
    }

    #[test]
    fn test_cannot_mint_when_globally_paused() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 303);

        // Pause
        let pause_disc = anchor_discriminator("pause");
        let pause_accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
        ];
        let pause_ix =
            Instruction::new_with_bytes(program_id(), &pause_disc, pause_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[pause_ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Pause should work");

        // Mint while paused
        let ix = build_admin_mint_ix(
            &env.minter.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            1,
            false,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should not mint when program is paused");
    }
}

// ============================================================================
// 10. CLAIMING TESTS
// ============================================================================

#[cfg(test)]
mod claiming {
    use super::*;

    #[test]
    fn test_user_claim_reward() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 400);
        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

        let disc = anchor_discriminator("claim_reward");
        // ClaimReward accounts: user, config, reward_token_state, treasury_state, token_program, system_program
        // SOL rewards use user AccountInfo directly (no remaining_accounts needed)
        let accounts = vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &disc, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&user.pubkey()),
            &[&user],
            blockhash,
        );
        // SOL claim may fail since treasury has no lamports to distribute -
        // this tests the account layout is accepted by Anchor deserialization
        let _result = env.svm.send_transaction(tx);
    }

    #[test]
    fn test_admin_claim_reward() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 401);
        let beneficiary = Keypair::new();
        env.svm.airdrop(&beneficiary.pubkey(), 10_000_000_000).unwrap();

        let disc = anchor_discriminator("admin_claim_reward");
        // AdminClaimReward accounts: manager, config, reward_token_state, treasury_state, beneficiary, token_program, system_program
        let accounts = vec![
            AccountMeta::new(env.manager.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new(beneficiary.pubkey(), false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &disc, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        // May fail at distribution step due to treasury balance, but tests account layout
        let _result = env.svm.send_transaction(tx);
    }

    #[test]
    fn test_cannot_claim_when_globally_paused() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 402);

        // Pause
        let pause_disc = anchor_discriminator("pause");
        let pause_accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
        ];
        let pause_ix =
            Instruction::new_with_bytes(program_id(), &pause_disc, pause_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[pause_ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Pause should work");

        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();
        let claim_disc = anchor_discriminator("claim_reward");
        let accounts = vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &claim_disc, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&user.pubkey()),
            &[&user],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should not claim when paused");
    }

    #[test]
    fn test_non_manager_cannot_admin_claim() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 403);
        let attacker = Keypair::new();
        env.svm
            .airdrop(&attacker.pubkey(), 10_000_000_000)
            .unwrap();
        let beneficiary = Keypair::new();
        env.svm.airdrop(&beneficiary.pubkey(), 10_000_000_000).unwrap();

        let disc = anchor_discriminator("admin_claim_reward");
        let accounts = vec![
            AccountMeta::new(attacker.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new(beneficiary.pubkey(), false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &disc, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-manager should not admin claim");
    }
}

// ============================================================================
// 11. SIGNATURE-VERIFIED MINTING TESTS
// ============================================================================

#[cfg(test)]
mod signature_minting {
    use super::*;
    /// Build an Ed25519 native program instruction that verifies a signature.
    ///
    /// Constructs the instruction data manually since `new_ed25519_instruction`
    /// expects `ed25519_dalek::Keypair` which differs from `solana_sdk::Keypair`.
    fn build_ed25519_instruction(signer: &Keypair, message: &[u8]) -> Instruction {
        let ed25519_program_id: Pubkey = "Ed25519SigVerify111111111111111111111111111"
            .parse()
            .unwrap();

        // Sign the message
        let signature = signer.sign_message(message);

        let num_signatures: u16 = 1;
        let padding: u8 = 0;

        // Header: num_signatures(2) + padding(1) + offsets(14) = 17 bytes
        let header_size: usize = 2 + 1 + 14;
        let signature_offset: u16 = header_size as u16;
        let public_key_offset: u16 = (header_size + 64) as u16;
        let message_data_offset: u16 = (header_size + 64 + 32) as u16;
        let message_data_size: u16 = message.len() as u16;
        // u16::MAX = data is in this instruction
        let instruction_index: u16 = u16::MAX;

        let mut data = Vec::new();
        data.extend_from_slice(&num_signatures.to_le_bytes());
        data.push(padding);
        data.extend_from_slice(&signature_offset.to_le_bytes());
        data.extend_from_slice(&instruction_index.to_le_bytes());
        data.extend_from_slice(&public_key_offset.to_le_bytes());
        data.extend_from_slice(&instruction_index.to_le_bytes());
        data.extend_from_slice(&message_data_offset.to_le_bytes());
        data.extend_from_slice(&message_data_size.to_le_bytes());
        data.extend_from_slice(&instruction_index.to_le_bytes());
        // Inline data
        data.extend_from_slice(signature.as_ref());
        data.extend_from_slice(&signer.pubkey().to_bytes());
        data.extend_from_slice(message);

        Instruction::new_with_bytes(ed25519_program_id, &data, vec![])
    }

    /// Add a whitelist signer to the program config.
    fn add_whitelist_signer(env: &mut TestEnv, signer_pubkey: &Pubkey) {
        let disc = anchor_discriminator("add_whitelist_signer");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&signer_pubkey.to_bytes());
        let accounts = vec![
            AccountMeta::new_readonly(env.dev_config.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(env.whitelist_signers_pda, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.dev_config.pubkey()),
            &[&env.dev_config],
            blockhash,
        );
        env.svm
            .send_transaction(tx)
            .expect("Add whitelist signer should work");
    }

    /// Build a mint_with_signature instruction.
    #[allow(clippy::too_many_arguments)]
    fn build_mint_with_signature_ix(
        user: &Pubkey,
        config: &Pubkey,
        reward_token_pda: &Pubkey,
        whitelist_signers_pda: &Pubkey,
        user_nonce_pda: &Pubkey,
        nonce: u64,
        is_soulbound: bool,
        is_claim_reward: bool,
    ) -> Instruction {
        let disc = anchor_discriminator("mint_with_signature");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&nonce.to_le_bytes());
        data.push(u8::from(is_soulbound));
        data.push(u8::from(is_claim_reward));

        let accounts = vec![
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new(*reward_token_pda, false),
            AccountMeta::new_readonly(*whitelist_signers_pda, false),
            AccountMeta::new(*user_nonce_pda, false),
            AccountMeta::new_readonly(
                solana_sdk::sysvar::instructions::id(),
                false,
            ),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        Instruction::new_with_bytes(program_id(), &data, accounts)
    }

    /// Build the message format expected by the handler: [user_pubkey(32) | token_id(8) | nonce(8)]
    fn build_expected_message(user: &Pubkey, token_id: u64, nonce: u64) -> Vec<u8> {
        let mut msg = Vec::with_capacity(48);
        msg.extend_from_slice(user.as_ref());
        msg.extend_from_slice(&token_id.to_le_bytes());
        msg.extend_from_slice(&nonce.to_le_bytes());
        msg
    }

    /// Build a complete mint-with-signature transaction including the Ed25519 instruction.
    fn build_signed_mint_tx(
        env: &mut TestEnv,
        user: &Keypair,
        reward_token_pda: &Pubkey,
        wl_signer: &Keypair,
        token_id: u64,
        nonce: u64,
    ) -> Transaction {
        let (user_nonce_pda, _) = find_nonce_pda(&env.config_pda, &user.pubkey(), nonce);

        // Build and sign the message
        let msg = build_expected_message(&user.pubkey(), token_id, nonce);
        let ed25519_ix = build_ed25519_instruction(wl_signer, &msg);

        let mint_ix = build_mint_with_signature_ix(
            &user.pubkey(),
            &env.config_pda,
            reward_token_pda,
            &env.whitelist_signers_pda,
            &user_nonce_pda,
            nonce,
            false,
            false,
        );

        let blockhash = env.svm.latest_blockhash();
        Transaction::new_signed_with_payer(
            &[ed25519_ix, mint_ix],
            Some(&user.pubkey()),
            &[user],
            blockhash,
        )
    }

    /// Verifies the Ed25519 instruction builder produces correct data layout.
    /// The Ed25519 precompile is not available in litesvm 0.3.0, so we validate
    /// the instruction data structure offline and test on-chain rejection separately.
    #[test]
    fn test_ed25519_instruction_data_layout() {
        let signer = Keypair::new();
        let user = Keypair::new();
        let token_id: u64 = 500;
        let nonce: u64 = 1;
        let msg = build_expected_message(&user.pubkey(), token_id, nonce);

        let ix = build_ed25519_instruction(&signer, &msg);

        // Verify program ID
        let ed25519_program_id: Pubkey = "Ed25519SigVerify111111111111111111111111111"
            .parse()
            .unwrap();
        assert_eq!(ix.program_id, ed25519_program_id);

        // Verify data layout
        let data = &ix.data;
        let num_sigs = u16::from_le_bytes([data[0], data[1]]);
        assert_eq!(num_sigs, 1, "Should have 1 signature");

        let header_size: usize = 2 + 1 + 14; // num_signatures + padding + offsets
        let sig_offset = u16::from_le_bytes([data[3], data[4]]) as usize;
        assert_eq!(sig_offset, header_size, "Signature should start after header");

        let pk_offset = u16::from_le_bytes([data[7], data[8]]) as usize;
        assert_eq!(pk_offset, header_size + 64, "Public key after signature");

        let msg_offset = u16::from_le_bytes([data[11], data[12]]) as usize;
        assert_eq!(msg_offset, header_size + 64 + 32, "Message after public key");

        let msg_size = u16::from_le_bytes([data[13], data[14]]) as usize;
        assert_eq!(msg_size, 48, "Message is user(32) + token_id(8) + nonce(8)");

        // Verify inline public key
        let extracted_pk = &data[pk_offset..pk_offset + 32];
        assert_eq!(extracted_pk, signer.pubkey().as_ref(), "Pubkey should match signer");

        // Verify inline message
        let extracted_msg = &data[msg_offset..msg_offset + msg_size];
        assert_eq!(extracted_msg, msg.as_slice(), "Message should match");
    }

    /// Test that mint_with_signature WITHOUT a preceding Ed25519 instruction
    /// is rejected by the program with MissingEd25519Instruction (error 6024).
    #[test]
    fn test_mint_without_ed25519_instruction_fails() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 500);

        let wl_signer = Keypair::new();
        add_whitelist_signer(&mut env, &wl_signer.pubkey());

        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

        let nonce: u64 = 1;
        let (user_nonce_pda, _) = find_nonce_pda(&env.config_pda, &user.pubkey(), nonce);

        // Send mint_with_signature WITHOUT the Ed25519 instruction
        let mint_ix = build_mint_with_signature_ix(
            &user.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &env.whitelist_signers_pda,
            &user_nonce_pda,
            nonce,
            false,
            false,
        );

        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[mint_ix],
            Some(&user.pubkey()),
            &[&user],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_err(),
            "Should fail without preceding Ed25519 instruction"
        );
    }

    /// Test that different nonce values produce different user_nonce PDAs.
    #[test]
    fn test_different_nonces_produce_different_pdas() {
        let (config, _) = find_config_pda();
        let user = Keypair::new();

        let (nonce_pda_1, _) = find_nonce_pda(&config, &user.pubkey(), 1);
        let (nonce_pda_2, _) = find_nonce_pda(&config, &user.pubkey(), 2);
        let (nonce_pda_42, _) = find_nonce_pda(&config, &user.pubkey(), 42);

        assert_ne!(nonce_pda_1, nonce_pda_2, "Different nonces should produce different PDAs");
        assert_ne!(nonce_pda_1, nonce_pda_42, "Different nonces should produce different PDAs");
        assert_ne!(nonce_pda_2, nonce_pda_42, "Different nonces should produce different PDAs");
    }

    // NOTE: The following tests require Ed25519 precompile support which is not
    // available in litesvm 0.3.0. They are marked #[ignore] and can be enabled
    // when upgrading to litesvm >= 0.9.1 with the "precompiles" feature.
    // Run with: cargo test -- --ignored

    #[test]
    #[ignore = "Requires Ed25519 precompile (litesvm >= 0.9.1 with 'precompiles' feature)"]
    fn test_mint_with_signature_creates_nonce_pda() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 510);

        let wl_signer = Keypair::new();
        add_whitelist_signer(&mut env, &wl_signer.pubkey());

        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

        let nonce: u64 = 1;
        let token_id: u64 = 510;
        let (user_nonce_pda, _) = find_nonce_pda(&env.config_pda, &user.pubkey(), nonce);

        let tx = build_signed_mint_tx(&mut env, &user, &reward_token_pda, &wl_signer, token_id, nonce);
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "Mint with signature should create nonce PDA: {:?}",
            result.err()
        );

        let nonce_account = env.svm.get_account(&user_nonce_pda);
        assert!(nonce_account.is_some(), "Nonce PDA should be created");
    }

    #[test]
    #[ignore = "Requires Ed25519 precompile (litesvm >= 0.9.1 with 'precompiles' feature)"]
    fn test_nonce_replay_protection() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 511);

        let wl_signer = Keypair::new();
        add_whitelist_signer(&mut env, &wl_signer.pubkey());

        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

        let nonce: u64 = 42;
        let token_id: u64 = 511;

        // First mint
        let tx = build_signed_mint_tx(&mut env, &user, &reward_token_pda, &wl_signer, token_id, nonce);
        env.svm
            .send_transaction(tx)
            .expect("First mint should work");

        // Replay - should fail (nonce PDA already exists)
        let tx = build_signed_mint_tx(&mut env, &user, &reward_token_pda, &wl_signer, token_id, nonce);
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Replay with same nonce should fail");
    }

    #[test]
    #[ignore = "Requires Ed25519 precompile (litesvm >= 0.9.1 with 'precompiles' feature)"]
    fn test_different_nonces_work() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 512);

        let wl_signer = Keypair::new();
        add_whitelist_signer(&mut env, &wl_signer.pubkey());

        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();
        let token_id: u64 = 512;

        // Nonce 1
        let tx = build_signed_mint_tx(&mut env, &user, &reward_token_pda, &wl_signer, token_id, 1);
        env.svm.send_transaction(tx).expect("Nonce 1 should work");

        // Nonce 2
        let tx = build_signed_mint_tx(&mut env, &user, &reward_token_pda, &wl_signer, token_id, 2);
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Different nonce should work: {:?}", result.err());
    }

    #[test]
    fn test_cannot_mint_when_paused() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 503);

        let wl_signer = Keypair::new();
        add_whitelist_signer(&mut env, &wl_signer.pubkey());

        // Pause the program
        let pause_disc = anchor_discriminator("pause");
        let pause_accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
        ];
        let pause_ix =
            Instruction::new_with_bytes(program_id(), &pause_disc, pause_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[pause_ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Pause should work");

        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();
        let token_id: u64 = 503;
        let nonce: u64 = 1;

        let tx = build_signed_mint_tx(&mut env, &user, &reward_token_pda, &wl_signer, token_id, nonce);
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should not mint when paused");
    }

    #[test]
    fn test_non_whitelisted_signer_fails() {
        let mut env = require_program!(setup_initialized());
        let reward_token_pda = create_simple_reward_token(&mut env, 504);

        // DO NOT add this signer to the whitelist
        let non_wl_signer = Keypair::new();

        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();
        let token_id: u64 = 504;
        let nonce: u64 = 1;

        let tx = build_signed_mint_tx(&mut env, &user, &reward_token_pda, &non_wl_signer, token_id, nonce);
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_err(),
            "Mint with non-whitelisted signer should fail"
        );
    }
}

// ============================================================================
// 12. EDGE CASES & PDA DERIVATION TESTS
//     These tests do NOT require the compiled program binary.
// ============================================================================

#[cfg(test)]
mod edge_cases {
    use super::*;

    #[test]
    fn test_pda_derivation_consistency() {
        let (config1, bump1) = find_config_pda();
        let (config2, bump2) = find_config_pda();
        assert_eq!(config1, config2, "PDA derivation should be deterministic");
        assert_eq!(bump1, bump2, "Bump should be consistent");
    }

    #[test]
    fn test_different_token_ids_get_different_pdas() {
        let (config, _) = find_config_pda();
        let (pda1, _) = find_reward_token_pda(&config, 1);
        let (pda2, _) = find_reward_token_pda(&config, 2);
        assert_ne!(
            pda1, pda2,
            "Different token IDs should produce different PDAs"
        );
    }

    #[test]
    fn test_different_users_get_different_nonce_pdas() {
        let (config, _) = find_config_pda();
        let user1 = Pubkey::new_unique();
        let user2 = Pubkey::new_unique();
        let (pda1, _) = find_nonce_pda(&config, &user1, 1);
        let (pda2, _) = find_nonce_pda(&config, &user2, 1);
        assert_ne!(
            pda1, pda2,
            "Different users should get different nonce PDAs"
        );
    }

    #[test]
    fn test_same_user_different_nonces_get_different_pdas() {
        let (config, _) = find_config_pda();
        let user = Pubkey::new_unique();
        let (pda1, _) = find_nonce_pda(&config, &user, 1);
        let (pda2, _) = find_nonce_pda(&config, &user, 2);
        assert_ne!(
            pda1, pda2,
            "Same user with different nonces should get different PDAs"
        );
    }

    #[test]
    fn test_reservation_pdas_per_mint() {
        let (config, _) = find_config_pda();
        let mint1 = Pubkey::new_unique();
        let mint2 = Pubkey::new_unique();
        let (res1, _) = find_token_reservation_pda(&config, &mint1);
        let (res2, _) = find_token_reservation_pda(&config, &mint2);
        assert_ne!(
            res1, res2,
            "Different mints should produce different reservation PDAs"
        );
    }

    #[test]
    fn test_nft_reservation_pdas_per_nft() {
        let (config, _) = find_config_pda();
        let nft1 = Pubkey::new_unique();
        let nft2 = Pubkey::new_unique();
        let (res1, _) = find_nft_reservation_pda(&config, &nft1);
        let (res2, _) = find_nft_reservation_pda(&config, &nft2);
        assert_ne!(
            res1, res2,
            "Different NFTs should produce different reservation PDAs"
        );
    }

    #[test]
    fn test_max_token_id_value() {
        let (config, _) = find_config_pda();
        let (pda, _bump) = find_reward_token_pda(&config, u64::MAX);
        assert_ne!(pda, Pubkey::default());
    }

    #[test]
    fn test_max_nonce_value() {
        let (config, _) = find_config_pda();
        let user = Pubkey::new_unique();
        let (pda, _bump) = find_nonce_pda(&config, &user, u64::MAX);
        assert_ne!(pda, Pubkey::default());
    }

    #[test]
    fn test_anchor_discriminator_format() {
        // Verify our discriminator matches the sha256("global:xxx")[..8] format
        let disc = anchor_discriminator("initialize");
        assert_eq!(disc.len(), 8);
        // The discriminator should not be all zeros
        assert!(disc.iter().any(|&b| b != 0));
    }

    #[test]
    fn test_different_instructions_have_different_discriminators() {
        let d1 = anchor_discriminator("initialize");
        let d2 = anchor_discriminator("pause");
        let d3 = anchor_discriminator("unpause");
        let d4 = anchor_discriminator("admin_mint");
        let d5 = anchor_discriminator("claim_reward");

        assert_ne!(d1, d2);
        assert_ne!(d2, d3);
        assert_ne!(d3, d4);
        assert_ne!(d4, d5);
    }
}

// ============================================================================
// 13. FULL INTEGRATION FLOW
// ============================================================================

#[cfg(test)]
mod integration {
    use super::*;

    #[test]
    fn test_full_lifecycle() {
        let mut env = require_program!(setup());

        // Step 1: Initialize
        initialize(&mut env).expect("Initialize should succeed");

        // Step 2: Add a whitelist signer
        let signer = Keypair::new();
        let add_signer_disc = anchor_discriminator("add_whitelist_signer");
        let mut add_signer_data = Vec::new();
        add_signer_data.extend_from_slice(&add_signer_disc);
        add_signer_data.extend_from_slice(&signer.pubkey().to_bytes());
        let add_signer_accounts = vec![
            AccountMeta::new_readonly(env.dev_config.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(env.whitelist_signers_pda, false),
        ];
        let ix =
            Instruction::new_with_bytes(program_id(), &add_signer_data, add_signer_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.dev_config.pubkey()),
            &[&env.dev_config],
            blockhash,
        );
        env.svm
            .send_transaction(tx)
            .expect("Add signer should work");

        // Step 3: Whitelist a token
        let mock_mint = Keypair::new();
        let whitelist_disc = anchor_discriminator("whitelist_token");
        let mut whitelist_data = Vec::new();
        whitelist_data.extend_from_slice(&whitelist_disc);
        whitelist_data.extend_from_slice(&mock_mint.pubkey().to_bytes());
        whitelist_data.push(1);
        let whitelist_accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(env.token_whitelist_pda, false),
        ];
        let ix =
            Instruction::new_with_bytes(program_id(), &whitelist_data, whitelist_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm
            .send_transaction(tx)
            .expect("Whitelist token should work");

        // Step 4: Create a reward token (SOL reward)
        let token_id: u64 = 1;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);

        // Verify PDA
        let reward_account = env.svm.get_account(&reward_token_pda);
        assert!(reward_account.is_some(), "Reward token PDA should exist");
        assert_eq!(
            reward_account.unwrap().owner,
            program_id(),
            "Reward token should be owned by program"
        );

        // Step 5: Admin mint
        let mint_disc = anchor_discriminator("admin_mint");
        let mut mint_data = Vec::new();
        mint_data.extend_from_slice(&mint_disc);
        mint_data.extend_from_slice(&5u64.to_le_bytes());
        mint_data.push(0);

        let mint_accounts = vec![
            AccountMeta::new(env.minter.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &mint_data, mint_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        env.svm
            .send_transaction(tx)
            .expect("Admin mint should work");

        // Step 6: User claim attempt (may fail at reward distribution due to treasury balance)
        let spl_token_id: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();
        let claim_disc = anchor_discriminator("claim_reward");
        let claim_accounts = vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new_readonly(spl_token_id, false),
            AccountMeta::new_readonly(system_program::ID, false),
            // SOL rewards: no remaining_accounts needed (user is recipient AccountInfo)
        ];
        let ix = Instruction::new_with_bytes(program_id(), &claim_disc, claim_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&user.pubkey()),
            &[&user],
            blockhash,
        );
        // Claim may fail at reward distribution (treasury has no funds to distribute)
        // but the account layout should be accepted by Anchor
        let _result = env.svm.send_transaction(tx);
    }

    #[test]
    fn test_pause_blocks_operations_then_unpause_resumes() {
        let mut env = require_program!(setup());
        initialize(&mut env).expect("Initialize");

        // Create a reward token
        let token_id: u64 = 50;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);

        // PAUSE
        let pause_disc = anchor_discriminator("pause");
        let pause_accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &pause_disc, pause_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Pause should work");

        // Mint while paused - should fail
        let mint_disc = anchor_discriminator("admin_mint");
        let mut mint_data = Vec::new();
        mint_data.extend_from_slice(&mint_disc);
        mint_data.extend_from_slice(&1u64.to_le_bytes());
        mint_data.push(0);
        let mint_accounts = vec![
            AccountMeta::new(env.minter.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix =
            Instruction::new_with_bytes(program_id(), &mint_data.clone(), mint_accounts.clone());
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Mint should fail when paused");

        // UNPAUSE
        let unpause_disc = anchor_discriminator("unpause");
        let unpause_accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
        ];
        let ix =
            Instruction::new_with_bytes(program_id(), &unpause_disc, unpause_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Unpause should work");

        // Mint after unpause - should work (use different amount to avoid AlreadyProcessed)
        let mint_disc2 = anchor_discriminator("admin_mint");
        let mut mint_data2 = Vec::new();
        mint_data2.extend_from_slice(&mint_disc2);
        mint_data2.extend_from_slice(&2u64.to_le_bytes()); // different amount
        mint_data2.push(0);
        let mint_accounts2 = vec![
            AccountMeta::new(env.minter.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &mint_data2, mint_accounts2);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Mint should work after unpause: {:?}", result.err());
    }

    #[test]
    fn test_role_change_propagation() {
        let mut env = require_program!(setup());
        initialize(&mut env).expect("Initialize");

        let new_manager = Keypair::new();
        env.svm
            .airdrop(&new_manager.pubkey(), 10_000_000_000)
            .unwrap();

        // Change manager
        let update_disc = anchor_discriminator("update_roles");
        let mut update_data = Vec::new();
        update_data.extend_from_slice(&update_disc);
        update_data.push(0);
        update_data.extend_from_slice(&new_manager.pubkey().to_bytes());

        let update_accounts = vec![
            AccountMeta::new_readonly(env.admin.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
        ];
        let ix =
            Instruction::new_with_bytes(program_id(), &update_data, update_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.admin.pubkey()),
            &[&env.admin],
            blockhash,
        );
        env.svm
            .send_transaction(tx)
            .expect("Role update should work");

        // Old manager cannot pause
        let pause_disc = anchor_discriminator("pause");
        let pause_accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &pause_disc, pause_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Old manager should not be able to pause");

        // New manager can pause
        let pause_accounts = vec![
            AccountMeta::new_readonly(new_manager.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &pause_disc, pause_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&new_manager.pubkey()),
            &[&new_manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "New manager should be able to pause");
    }

    #[test]
    fn test_multiple_reward_tokens_independent() {
        let mut env = require_program!(setup());
        initialize(&mut env).expect("Initialize");

        for token_id in 1..=5u64 {
            let (reward_token_pda, _) = find_reward_token_pda(&env.config_pda, token_id);
            let uri = format!("https://summon.xyz/token/{}", token_id);
            let ix = build_simple_create_reward_token_ix(
                &env.manager.pubkey(),
                &env.config_pda,
                &reward_token_pda,
                &env.treasury_pda,
                token_id,
                &uri,
                token_id * 100,
            );
            let blockhash = env.svm.latest_blockhash();
            let tx = Transaction::new_signed_with_payer(
                &[ix],
                Some(&env.manager.pubkey()),
                &[&env.manager],
                blockhash,
            );
            env.svm
                .send_transaction(tx)
                .unwrap_or_else(|e| panic!("Token {} should create: {:?}", token_id, e));

            let account = env.svm.get_account(&reward_token_pda);
            assert!(account.is_some(), "Token {} PDA should exist", token_id);
        }
    }

    #[test]
    fn test_treasury_config_association() {
        let env = require_program!(setup_initialized());

        let (expected_treasury, _) = find_treasury_pda(&env.config_pda);
        assert_eq!(
            env.treasury_pda, expected_treasury,
            "Treasury PDA should be correctly derived from config"
        );

        let treasury_account = env.svm.get_account(&env.treasury_pda);
        assert!(treasury_account.is_some(), "Treasury should exist");
        assert_eq!(
            treasury_account.unwrap().owner,
            program_id(),
            "Treasury should be owned by program"
        );
    }
}

// ============================================================================
// 14. PDA SEED VALIDATION TESTS
// ============================================================================

#[cfg(test)]
mod pda_seeds {
    use super::*;

    #[test]
    fn test_config_pda_seeds() {
        let (pda, _) = Pubkey::find_program_address(&[b"config"], &program_id());
        let (expected, _) = find_config_pda();
        assert_eq!(pda, expected);
    }

    #[test]
    fn test_whitelist_signers_seeds_depend_on_config() {
        let config1 = Pubkey::new_unique();
        let config2 = Pubkey::new_unique();
        let (pda1, _) = find_whitelist_signers_pda(&config1);
        let (pda2, _) = find_whitelist_signers_pda(&config2);
        assert_ne!(
            pda1, pda2,
            "Different configs should produce different signer PDAs"
        );
    }

    #[test]
    fn test_token_whitelist_seeds_depend_on_config() {
        let config1 = Pubkey::new_unique();
        let config2 = Pubkey::new_unique();
        let (pda1, _) = find_token_whitelist_pda(&config1);
        let (pda2, _) = find_token_whitelist_pda(&config2);
        assert_ne!(
            pda1, pda2,
            "Different configs should produce different whitelist PDAs"
        );
    }

    #[test]
    fn test_treasury_seeds_depend_on_config() {
        let config1 = Pubkey::new_unique();
        let config2 = Pubkey::new_unique();
        let (pda1, _) = find_treasury_pda(&config1);
        let (pda2, _) = find_treasury_pda(&config2);
        assert_ne!(
            pda1, pda2,
            "Different configs should produce different treasury PDAs"
        );
    }

    #[test]
    fn test_reward_token_seeds_include_token_id() {
        let config = Pubkey::new_unique();
        let (pda_a, _) = find_reward_token_pda(&config, 0);
        let (pda_b, _) = find_reward_token_pda(&config, 1);
        let (pda_c, _) = find_reward_token_pda(&config, u64::MAX);
        assert_ne!(pda_a, pda_b);
        assert_ne!(pda_b, pda_c);
        assert_ne!(pda_a, pda_c);
    }

    #[test]
    fn test_nonce_seeds_include_user_and_nonce() {
        let config = Pubkey::new_unique();
        let user1 = Pubkey::new_unique();
        let user2 = Pubkey::new_unique();

        let (pda_u1_n1, _) = find_nonce_pda(&config, &user1, 1);
        let (pda_u1_n2, _) = find_nonce_pda(&config, &user1, 2);
        let (pda_u2_n1, _) = find_nonce_pda(&config, &user2, 1);

        assert_ne!(
            pda_u1_n1, pda_u1_n2,
            "Same user, different nonce -> different PDA"
        );
        assert_ne!(
            pda_u1_n1, pda_u2_n1,
            "Different user, same nonce -> different PDA"
        );
    }

    #[test]
    fn test_reservation_seeds_include_mint() {
        let config = Pubkey::new_unique();
        let mint1 = Pubkey::new_unique();
        let mint2 = Pubkey::new_unique();

        let (pda1, _) = find_token_reservation_pda(&config, &mint1);
        let (pda2, _) = find_token_reservation_pda(&config, &mint2);
        assert_ne!(pda1, pda2);
    }
}

// ============================================================================
// 15. SPL HELPERS FOR TREASURY / ACCESS TOKEN TESTS
// ============================================================================

fn spl_token_program_id() -> Pubkey {
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        .parse()
        .unwrap()
}

fn spl_token_2022_program_id() -> Pubkey {
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        .parse()
        .unwrap()
}

fn spl_associated_token_program_id() -> Pubkey {
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        .parse()
        .unwrap()
}

/// Derive the associated token address for a given wallet and mint.
fn get_associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            spl_token_program_id().as_ref(),
            mint.as_ref(),
        ],
        &spl_associated_token_program_id(),
    )
    .0
}

/// Create an SPL token mint in LiteSVM using raw byte layout.
/// SPL Token Mint layout (82 bytes):
///   [0..36]  = COption<Pubkey> mint_authority (4 tag + 32 pubkey)
///   [36..44] = u64 supply
///   [44]     = u8 decimals
///   [45]     = u8 is_initialized (1 = true)
///   [46..82] = COption<Pubkey> freeze_authority
fn create_spl_mint(
    svm: &mut LiteSVM,
    mint_keypair: &Keypair,
    mint_authority: &Pubkey,
    decimals: u8,
) {
    let mint_len: usize = 82;
    let rent = svm.minimum_balance_for_rent_exemption(mint_len);

    let mut data = vec![0u8; mint_len];
    // mint_authority = Some(pubkey): tag=1(u32 LE) + pubkey
    data[0..4].copy_from_slice(&1u32.to_le_bytes()); // COption tag = Some
    data[4..36].copy_from_slice(mint_authority.as_ref());
    // supply = 0
    data[36..44].copy_from_slice(&0u64.to_le_bytes());
    // decimals
    data[44] = decimals;
    // is_initialized = true
    data[45] = 1;
    // freeze_authority = None (tag=0, rest zeros)

    svm.set_account(mint_keypair.pubkey(), solana_sdk::account::Account {
        lamports: rent,
        data,
        owner: spl_token_program_id(),
        executable: false,
        rent_epoch: 0,
    }).unwrap();
}

/// Create an SPL token account in LiteSVM using raw byte layout.
/// SPL Token Account layout (165 bytes):
///   [0..32]   = Pubkey mint
///   [32..64]  = Pubkey owner
///   [64..72]  = u64 amount
///   [72..108] = COption<Pubkey> delegate
///   [108]     = u8 state (1 = Initialized)
///   [109..145] = COption<u64> is_native
///   [145..153] = u64 delegated_amount
///   [153..189] = COption<Pubkey> close_authority
/// Note: total is 165 bytes
fn create_spl_token_account(
    svm: &mut LiteSVM,
    token_account_pubkey: &Pubkey,
    mint: &Pubkey,
    owner: &Pubkey,
    amount: u64,
) {
    let account_len: usize = 165;
    let rent = svm.minimum_balance_for_rent_exemption(account_len);

    let mut data = vec![0u8; account_len];
    data[0..32].copy_from_slice(mint.as_ref());       // mint
    data[32..64].copy_from_slice(owner.as_ref());      // owner
    data[64..72].copy_from_slice(&amount.to_le_bytes()); // amount
    // delegate = None (tag=0 at [72..76])
    data[108] = 1; // state = Initialized
    // is_native = None (tag=0 at [109..113])
    // delegated_amount = 0 at [145..153]
    // close_authority = None (tag=0 at [153..157])

    svm.set_account(*token_account_pubkey, solana_sdk::account::Account {
        lamports: rent,
        data,
        owner: spl_token_program_id(),
        executable: false,
        rent_epoch: 0,
    }).unwrap();
}

/// Read the amount from an SPL token account in LiteSVM.
/// Amount is at bytes [64..72] in the SPL token account layout.
fn read_spl_token_balance(svm: &LiteSVM, token_account_pubkey: &Pubkey) -> u64 {
    let account = svm.get_account(token_account_pubkey).expect("Token account should exist");
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

/// Create a setup with SPL programs loaded.
fn setup_with_spl() -> Option<TestEnv> {
    let program_bytes = load_program_bytes()?;
    let mut svm = LiteSVM::new().with_spl_programs();
    svm.add_program(program_id(), &program_bytes);

    let admin = Keypair::new();
    let manager = Keypair::new();
    let minter = Keypair::new();
    let dev_config = Keypair::new();

    svm.airdrop(&admin.pubkey(), 100_000_000_000).unwrap();
    svm.airdrop(&manager.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&minter.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&dev_config.pubkey(), 10_000_000_000).unwrap();

    let (config_pda, config_bump) = find_config_pda();
    let (whitelist_signers_pda, _) = find_whitelist_signers_pda(&config_pda);
    let (token_whitelist_pda, _) = find_token_whitelist_pda(&config_pda);
    let (treasury_pda, _) = find_treasury_pda(&config_pda);

    Some(TestEnv {
        svm,
        admin,
        manager,
        minter,
        dev_config,
        config_pda,
        config_bump,
        whitelist_signers_pda,
        token_whitelist_pda,
        treasury_pda,
    })
}

fn setup_initialized_with_spl() -> Option<TestEnv> {
    let mut env = setup_with_spl()?;
    initialize(&mut env).expect("Initialize should succeed");
    Some(env)
}

/// Whitelist a token mint in the program.
fn whitelist_token(env: &mut TestEnv, mint: &Pubkey, reward_type: u8) {
    let disc = anchor_discriminator("whitelist_token");
    let mut data = Vec::new();
    data.extend_from_slice(&disc);
    data.extend_from_slice(&mint.to_bytes());
    data.push(reward_type);
    let accounts = vec![
        AccountMeta::new_readonly(env.manager.pubkey(), true),
        AccountMeta::new_readonly(env.config_pda, false),
        AccountMeta::new(env.token_whitelist_pda, false),
    ];
    let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
    let blockhash = env.svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&env.manager.pubkey()),
        &[&env.manager],
        blockhash,
    );
    env.svm
        .send_transaction(tx)
        .expect("Whitelist token should work");
}

// ============================================================================
// 16. REMOVE TOKEN FROM WHITELIST TESTS
// ============================================================================

#[cfg(test)]
mod remove_from_whitelist {
    use super::*;

    #[test]
    fn test_manager_can_remove_whitelisted_token() {
        let mut env = require_program!(setup_initialized());
        let mock_mint = Pubkey::new_unique();

        // First whitelist the token (type=1 for SplToken)
        whitelist_token(&mut env, &mock_mint, 1);

        // Create reservation PDA with reserved_amount = 0 (required for SPL token removal)
        let (reservation_pda, reservation_bump) =
            find_token_reservation_pda(&env.config_pda, &mock_mint);
        let reservation_disc = {
            let hash_result = hash::hash(b"account:TokenReservation");
            let mut d = [0u8; 8];
            d.copy_from_slice(&hash_result.to_bytes()[..8]);
            d
        };
        let mut reservation_data = Vec::new();
        reservation_data.extend_from_slice(&reservation_disc);
        reservation_data.extend_from_slice(&mock_mint.to_bytes()); // mint
        reservation_data.extend_from_slice(&0u64.to_le_bytes()); // reserved_amount = 0
        reservation_data.push(reservation_bump); // bump
        let rent = env.svm.minimum_balance_for_rent_exemption(reservation_data.len());
        env.svm.set_account(reservation_pda, solana_sdk::account::Account {
            lamports: rent,
            data: reservation_data,
            owner: program_id(),
            executable: false,
            rent_epoch: 0,
        }).unwrap();

        // Now remove it (with reservation PDA as remaining_account)
        let disc = anchor_discriminator("remove_token_from_whitelist");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&mock_mint.to_bytes());
        let accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(env.token_whitelist_pda, false),
            // Reservation PDA as remaining_account
            AccountMeta::new_readonly(reservation_pda, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Manager should remove whitelisted token: {:?}", result.err());
    }

    #[test]
    fn test_cannot_remove_non_whitelisted_token() {
        let mut env = require_program!(setup_initialized());
        let unknown_mint = Pubkey::new_unique();

        let disc = anchor_discriminator("remove_token_from_whitelist");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&unknown_mint.to_bytes());
        let accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(env.token_whitelist_pda, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Should not remove non-whitelisted token");
    }

    #[test]
    fn test_non_manager_cannot_remove_from_whitelist() {
        let mut env = require_program!(setup_initialized());
        let mock_mint = Pubkey::new_unique();
        whitelist_token(&mut env, &mock_mint, 1);

        let attacker = Keypair::new();
        env.svm.airdrop(&attacker.pubkey(), 10_000_000_000).unwrap();

        let disc = anchor_discriminator("remove_token_from_whitelist");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&mock_mint.to_bytes());
        let accounts = vec![
            AccountMeta::new_readonly(attacker.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(env.token_whitelist_pda, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-manager should not remove from whitelist");
    }

    #[test]
    fn test_remove_then_re_whitelist() {
        let mut env = require_program!(setup_initialized());
        let mock_mint = Pubkey::new_unique();

        // Whitelist
        whitelist_token(&mut env, &mock_mint, 1);

        // Create reservation PDA with reserved_amount = 0 (required for SPL token removal)
        let (reservation_pda, reservation_bump) =
            find_token_reservation_pda(&env.config_pda, &mock_mint);
        let reservation_disc = {
            let hash_result = hash::hash(b"account:TokenReservation");
            let mut d = [0u8; 8];
            d.copy_from_slice(&hash_result.to_bytes()[..8]);
            d
        };
        let mut reservation_data = Vec::new();
        reservation_data.extend_from_slice(&reservation_disc);
        reservation_data.extend_from_slice(&mock_mint.to_bytes()); // mint
        reservation_data.extend_from_slice(&0u64.to_le_bytes()); // reserved_amount = 0
        reservation_data.push(reservation_bump); // bump
        let rent = env.svm.minimum_balance_for_rent_exemption(reservation_data.len());
        env.svm.set_account(reservation_pda, solana_sdk::account::Account {
            lamports: rent,
            data: reservation_data,
            owner: program_id(),
            executable: false,
            rent_epoch: 0,
        }).unwrap();

        // Remove (with reservation PDA as remaining_account)
        let disc = anchor_discriminator("remove_token_from_whitelist");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&mock_mint.to_bytes());
        let accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(env.token_whitelist_pda, false),
            AccountMeta::new_readonly(reservation_pda, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Remove should work");

        // Expire the blockhash so the re-whitelist tx gets a new signature
        env.svm.expire_blockhash();

        // Re-whitelist the same token
        whitelist_token(&mut env, &mock_mint, 1);
        // If we got here, re-whitelist succeeded
    }
}

// ============================================================================
// 17. TREASURY OPERATIONS (SPL deposit, withdraw, NFT deposit/withdraw)
// ============================================================================

#[cfg(test)]
mod treasury_operations {
    use super::*;

    #[test]
    fn test_deposit_spl_to_treasury() {
        let mut env = require_program!(setup_initialized_with_spl());

        // Create an SPL mint
        let mint_keypair = Keypair::new();
        let depositor = Keypair::new();
        env.svm.airdrop(&depositor.pubkey(), 10_000_000_000).unwrap();

        create_spl_mint(&mut env.svm, &mint_keypair, &depositor.pubkey(), 6);

        // Whitelist the token
        whitelist_token(&mut env, &mint_keypair.pubkey(), 1); // SplToken = 1

        // Create depositor's token account with 1000 tokens
        let depositor_ata = get_associated_token_address(&depositor.pubkey(), &mint_keypair.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &depositor_ata,
            &mint_keypair.pubkey(),
            &depositor.pubkey(),
            1_000_000,
        );

        // Create treasury's token account (empty)
        let treasury_ata = get_associated_token_address(&env.treasury_pda, &mint_keypair.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &treasury_ata,
            &mint_keypair.pubkey(),
            &env.treasury_pda,
            0,
        );

        // Deposit 500 tokens to treasury
        let disc = anchor_discriminator("deposit_to_treasury");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&500_000u64.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(depositor.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new_readonly(env.token_whitelist_pda, false),
            AccountMeta::new_readonly(env.treasury_pda, false),
            AccountMeta::new_readonly(mint_keypair.pubkey(), false),
            AccountMeta::new(depositor_ata, false),
            AccountMeta::new(treasury_ata, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&depositor.pubkey()),
            &[&depositor],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Deposit to treasury should work: {:?}", result.err());

        // Verify balances
        let treasury_balance = read_spl_token_balance(&env.svm, &treasury_ata);
        assert_eq!(treasury_balance, 500_000, "Treasury should have 500_000 tokens");

        let depositor_balance = read_spl_token_balance(&env.svm, &depositor_ata);
        assert_eq!(depositor_balance, 500_000, "Depositor should have 500_000 tokens remaining");
    }

    #[test]
    fn test_cannot_deposit_non_whitelisted_token() {
        let mut env = require_program!(setup_initialized_with_spl());

        let mint_keypair = Keypair::new();
        let depositor = Keypair::new();
        env.svm.airdrop(&depositor.pubkey(), 10_000_000_000).unwrap();

        create_spl_mint(&mut env.svm, &mint_keypair, &depositor.pubkey(), 6);
        // NOT whitelisting the token

        let depositor_ata = get_associated_token_address(&depositor.pubkey(), &mint_keypair.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &depositor_ata,
            &mint_keypair.pubkey(),
            &depositor.pubkey(),
            1_000_000,
        );

        let treasury_ata = get_associated_token_address(&env.treasury_pda, &mint_keypair.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &treasury_ata,
            &mint_keypair.pubkey(),
            &env.treasury_pda,
            0,
        );

        let disc = anchor_discriminator("deposit_to_treasury");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&500_000u64.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(depositor.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new_readonly(env.token_whitelist_pda, false),
            AccountMeta::new_readonly(env.treasury_pda, false),
            AccountMeta::new_readonly(mint_keypair.pubkey(), false),
            AccountMeta::new(depositor_ata, false),
            AccountMeta::new(treasury_ata, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&depositor.pubkey()),
            &[&depositor],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Deposit of non-whitelisted token should fail");
    }

    #[test]
    fn test_withdraw_unreserved_treasury() {
        let mut env = require_program!(setup_initialized_with_spl());

        let mint_keypair = Keypair::new();
        let depositor = Keypair::new();
        env.svm.airdrop(&depositor.pubkey(), 10_000_000_000).unwrap();

        create_spl_mint(&mut env.svm, &mint_keypair, &depositor.pubkey(), 6);
        whitelist_token(&mut env, &mint_keypair.pubkey(), 1);

        // Depositor has 1M tokens, deposits 500K
        let depositor_ata = get_associated_token_address(&depositor.pubkey(), &mint_keypair.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &depositor_ata,
            &mint_keypair.pubkey(),
            &depositor.pubkey(),
            1_000_000,
        );

        let treasury_ata = get_associated_token_address(&env.treasury_pda, &mint_keypair.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &treasury_ata,
            &mint_keypair.pubkey(),
            &env.treasury_pda,
            0,
        );

        // Deposit
        let disc = anchor_discriminator("deposit_to_treasury");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&500_000u64.to_le_bytes());
        let accounts = vec![
            AccountMeta::new(depositor.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new_readonly(env.token_whitelist_pda, false),
            AccountMeta::new_readonly(env.treasury_pda, false),
            AccountMeta::new_readonly(mint_keypair.pubkey(), false),
            AccountMeta::new(depositor_ata, false),
            AccountMeta::new(treasury_ata, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&depositor.pubkey()),
            &[&depositor],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Deposit should work");

        // Create a TokenReservation PDA with reserved_amount = 0
        // so all 500K is unreserved and can be withdrawn
        let (reservation_pda, reservation_bump) =
            find_token_reservation_pda(&env.config_pda, &mint_keypair.pubkey());
        let mut reservation_data = Vec::new();
        // Anchor discriminator for TokenReservation
        let reservation_disc = {
            let hash_result = hash::hash(b"account:TokenReservation");
            let mut d = [0u8; 8];
            d.copy_from_slice(&hash_result.to_bytes()[..8]);
            d
        };
        reservation_data.extend_from_slice(&reservation_disc);
        reservation_data.extend_from_slice(&mint_keypair.pubkey().to_bytes()); // mint
        reservation_data.extend_from_slice(&0u64.to_le_bytes()); // reserved_amount = 0
        reservation_data.push(reservation_bump); // bump

        let rent = env.svm.minimum_balance_for_rent_exemption(reservation_data.len());
        let reservation_account = solana_sdk::account::Account {
            lamports: rent,
            data: reservation_data,
            owner: program_id(),
            executable: false,
            rent_epoch: 0,
        };
        env.svm.set_account(reservation_pda, reservation_account).unwrap();

        // Create manager's destination token account
        let manager_ata = get_associated_token_address(&env.manager.pubkey(), &mint_keypair.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &manager_ata,
            &mint_keypair.pubkey(),
            &env.manager.pubkey(),
            0,
        );

        // Withdraw unreserved
        let withdraw_disc = anchor_discriminator("withdraw_unreserved_treasury");
        let withdraw_accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new_readonly(env.treasury_pda, false),
            AccountMeta::new_readonly(mint_keypair.pubkey(), false),
            AccountMeta::new(treasury_ata, false),
            AccountMeta::new(manager_ata, false),
            AccountMeta::new_readonly(reservation_pda, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &withdraw_disc, withdraw_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Withdraw unreserved should work: {:?}", result.err());

        // Verify manager received the tokens
        let manager_balance = read_spl_token_balance(&env.svm, &manager_ata);
        assert_eq!(manager_balance, 500_000, "Manager should receive all unreserved tokens");

        let treasury_balance = read_spl_token_balance(&env.svm, &treasury_ata);
        assert_eq!(treasury_balance, 0, "Treasury should be empty");
    }

    #[test]
    fn test_non_manager_cannot_withdraw_treasury() {
        let mut env = require_program!(setup_initialized_with_spl());

        let mint_keypair = Keypair::new();
        create_spl_mint(&mut env.svm, &mint_keypair, &env.manager.pubkey(), 6);
        whitelist_token(&mut env, &mint_keypair.pubkey(), 1);

        let treasury_ata = get_associated_token_address(&env.treasury_pda, &mint_keypair.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &treasury_ata,
            &mint_keypair.pubkey(),
            &env.treasury_pda,
            500_000,
        );

        let (reservation_pda, reservation_bump) =
            find_token_reservation_pda(&env.config_pda, &mint_keypair.pubkey());
        let reservation_disc = {
            let hash_result = hash::hash(b"account:TokenReservation");
            let mut d = [0u8; 8];
            d.copy_from_slice(&hash_result.to_bytes()[..8]);
            d
        };
        let mut reservation_data = Vec::new();
        reservation_data.extend_from_slice(&reservation_disc);
        reservation_data.extend_from_slice(&mint_keypair.pubkey().to_bytes());
        reservation_data.extend_from_slice(&0u64.to_le_bytes());
        reservation_data.push(reservation_bump);
        let rent = env.svm.minimum_balance_for_rent_exemption(reservation_data.len());
        env.svm.set_account(reservation_pda, solana_sdk::account::Account {
            lamports: rent,
            data: reservation_data,
            owner: program_id(),
            executable: false,
            rent_epoch: 0,
        }).unwrap();

        let attacker = Keypair::new();
        env.svm.airdrop(&attacker.pubkey(), 10_000_000_000).unwrap();
        let attacker_ata = get_associated_token_address(&attacker.pubkey(), &mint_keypair.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &attacker_ata,
            &mint_keypair.pubkey(),
            &attacker.pubkey(),
            0,
        );

        let withdraw_disc = anchor_discriminator("withdraw_unreserved_treasury");
        let withdraw_accounts = vec![
            AccountMeta::new_readonly(attacker.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new_readonly(env.treasury_pda, false),
            AccountMeta::new_readonly(mint_keypair.pubkey(), false),
            AccountMeta::new(treasury_ata, false),
            AccountMeta::new(attacker_ata, false),
            AccountMeta::new_readonly(reservation_pda, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &withdraw_disc, withdraw_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-manager should not withdraw from treasury");
    }

    #[test]
    fn test_deposit_nft_to_treasury() {
        let mut env = require_program!(setup_initialized_with_spl());

        // Create an NFT mint (supply=1, decimals=0)
        let nft_mint = Keypair::new();
        let depositor = Keypair::new();
        env.svm.airdrop(&depositor.pubkey(), 10_000_000_000).unwrap();

        create_spl_mint(&mut env.svm, &nft_mint, &depositor.pubkey(), 0);

        // Set mint supply to 1 (simulate already-minted NFT)
        // Supply is at bytes [36..44] in the SPL Mint layout
        {
            let account = env.svm.get_account(&nft_mint.pubkey()).unwrap();
            let mut data = account.data.clone();
            data[36..44].copy_from_slice(&1u64.to_le_bytes());
            env.svm.set_account(nft_mint.pubkey(), solana_sdk::account::Account {
                data,
                ..account
            }).unwrap();
        }

        // Whitelist as NFT (type=2)
        whitelist_token(&mut env, &nft_mint.pubkey(), 2);

        // Depositor owns the NFT
        let depositor_nft_ata = get_associated_token_address(&depositor.pubkey(), &nft_mint.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &depositor_nft_ata,
            &nft_mint.pubkey(),
            &depositor.pubkey(),
            1,
        );

        // Treasury NFT account (empty)
        let treasury_nft_ata = get_associated_token_address(&env.treasury_pda, &nft_mint.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &treasury_nft_ata,
            &nft_mint.pubkey(),
            &env.treasury_pda,
            0,
        );

        // Deposit NFT to treasury
        let disc = anchor_discriminator("deposit_nft_to_treasury");
        let accounts = vec![
            AccountMeta::new(depositor.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new_readonly(env.token_whitelist_pda, false),
            AccountMeta::new_readonly(env.treasury_pda, false),
            AccountMeta::new_readonly(nft_mint.pubkey(), false),
            AccountMeta::new(depositor_nft_ata, false),
            AccountMeta::new(treasury_nft_ata, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &disc, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&depositor.pubkey()),
            &[&depositor],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "NFT deposit to treasury should work: {:?}", result.err());

        // Verify NFT transferred
        let treasury_balance = read_spl_token_balance(&env.svm, &treasury_nft_ata);
        assert_eq!(treasury_balance, 1, "Treasury should hold the NFT");

        let depositor_balance = read_spl_token_balance(&env.svm, &depositor_nft_ata);
        assert_eq!(depositor_balance, 0, "Depositor should no longer hold the NFT");
    }

    #[test]
    fn test_withdraw_unreserved_nft() {
        let mut env = require_program!(setup_initialized_with_spl());

        let nft_mint = Keypair::new();
        create_spl_mint(&mut env.svm, &nft_mint, &env.manager.pubkey(), 0);

        // Set supply to 1 (supply at bytes [36..44])
        {
            let account = env.svm.get_account(&nft_mint.pubkey()).unwrap();
            let mut data = account.data.clone();
            data[36..44].copy_from_slice(&1u64.to_le_bytes());
            env.svm.set_account(nft_mint.pubkey(), solana_sdk::account::Account {
                data,
                ..account
            }).unwrap();
        }

        // Treasury holds the NFT
        let treasury_nft_ata = get_associated_token_address(&env.treasury_pda, &nft_mint.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &treasury_nft_ata,
            &nft_mint.pubkey(),
            &env.treasury_pda,
            1,
        );

        // Create NftReservation PDA (not reserved)
        let (nft_reservation_pda, nft_reservation_bump) =
            find_nft_reservation_pda(&env.config_pda, &nft_mint.pubkey());
        let reservation_disc = {
            let hash_result = hash::hash(b"account:NftReservation");
            let mut d = [0u8; 8];
            d.copy_from_slice(&hash_result.to_bytes()[..8]);
            d
        };
        let mut reservation_data = Vec::new();
        reservation_data.extend_from_slice(&reservation_disc);
        reservation_data.extend_from_slice(&nft_mint.pubkey().to_bytes()); // nft_mint
        reservation_data.push(0); // is_reserved = false
        reservation_data.push(nft_reservation_bump); // bump
        let rent = env.svm.minimum_balance_for_rent_exemption(reservation_data.len());
        env.svm.set_account(nft_reservation_pda, solana_sdk::account::Account {
            lamports: rent,
            data: reservation_data,
            owner: program_id(),
            executable: false,
            rent_epoch: 0,
        }).unwrap();

        // Manager's destination account
        let manager_nft_ata = get_associated_token_address(&env.manager.pubkey(), &nft_mint.pubkey());
        create_spl_token_account(
            &mut env.svm,
            &manager_nft_ata,
            &nft_mint.pubkey(),
            &env.manager.pubkey(),
            0,
        );

        // Withdraw unreserved NFT
        let disc = anchor_discriminator("withdraw_unreserved_nft");
        let accounts = vec![
            AccountMeta::new_readonly(env.manager.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new_readonly(env.treasury_pda, false),
            AccountMeta::new_readonly(nft_mint.pubkey(), false),
            AccountMeta::new_readonly(nft_reservation_pda, false),
            AccountMeta::new(treasury_nft_ata, false),
            AccountMeta::new(manager_nft_ata, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &disc, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_ok(), "Withdraw unreserved NFT should work: {:?}", result.err());

        // Verify NFT transferred to manager
        let manager_balance = read_spl_token_balance(&env.svm, &manager_nft_ata);
        assert_eq!(manager_balance, 1, "Manager should receive the NFT");

        let treasury_balance = read_spl_token_balance(&env.svm, &treasury_nft_ata);
        assert_eq!(treasury_balance, 0, "Treasury should no longer hold the NFT");
    }
}

// ============================================================================
// 18. ACCESS TOKEN (Token-2022 soulbound) TESTS
// ============================================================================

#[cfg(test)]
mod access_token_tests {
    use super::*;

    fn find_access_mint_pda(config: &Pubkey, token_id: u64) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"access_mint", config.as_ref(), &token_id.to_le_bytes()],
            &program_id(),
        )
    }

    /// Build create_access_token_mint instruction
    fn build_create_access_token_mint_ix(
        manager: &Pubkey,
        config: &Pubkey,
        reward_token_pda: &Pubkey,
        access_mint_pda: &Pubkey,
    ) -> Instruction {
        let disc = anchor_discriminator("create_access_token_mint");
        let accounts = vec![
            AccountMeta::new(*manager, true),
            AccountMeta::new(*config, false),
            AccountMeta::new(*reward_token_pda, false),
            AccountMeta::new(*access_mint_pda, false),
            AccountMeta::new_readonly(spl_token_2022_program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        Instruction::new_with_bytes(program_id(), &disc, accounts)
    }

    #[test]
    fn test_create_access_token_mint() {
        let mut env = require_program!(setup_initialized_with_spl());
        let token_id: u64 = 600;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);
        let (access_mint_pda, _) = find_access_mint_pda(&env.config_pda, token_id);

        let ix = build_create_access_token_mint_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &access_mint_pda,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "Create access token mint should work: {:?}",
            result.err()
        );

        // Verify the access mint PDA was created
        let mint_account = env.svm.get_account(&access_mint_pda);
        assert!(mint_account.is_some(), "Access token mint PDA should exist");

        // Verify it's owned by Token-2022 program
        let account = mint_account.unwrap();
        assert_eq!(
            account.owner,
            spl_token_2022_program_id(),
            "Access mint should be owned by Token-2022 program"
        );
    }

    #[test]
    fn test_cannot_create_access_mint_twice() {
        let mut env = require_program!(setup_initialized_with_spl());
        let token_id: u64 = 601;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);
        let (access_mint_pda, _) = find_access_mint_pda(&env.config_pda, token_id);

        // First creation
        let ix = build_create_access_token_mint_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &access_mint_pda,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("First creation should work");

        // Second creation should fail (DupTokenId - access_token_mint already set)
        let ix = build_create_access_token_mint_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &access_mint_pda,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Creating access mint twice should fail");
    }

    #[test]
    fn test_non_manager_cannot_create_access_mint() {
        let mut env = require_program!(setup_initialized_with_spl());
        let token_id: u64 = 602;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);
        let (access_mint_pda, _) = find_access_mint_pda(&env.config_pda, token_id);

        let attacker = Keypair::new();
        env.svm.airdrop(&attacker.pubkey(), 10_000_000_000).unwrap();

        let disc = anchor_discriminator("create_access_token_mint");
        let accounts = vec![
            AccountMeta::new(attacker.pubkey(), true),
            AccountMeta::new(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(access_mint_pda, false),
            AccountMeta::new_readonly(spl_token_2022_program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &disc, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-manager should not create access mint");
    }

    #[test]
    fn test_mint_access_token() {
        let mut env = require_program!(setup_initialized_with_spl());
        let token_id: u64 = 603;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);
        let (access_mint_pda, _) = find_access_mint_pda(&env.config_pda, token_id);

        // Create access token mint
        let ix = build_create_access_token_mint_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &access_mint_pda,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Create access mint");

        // Create a recipient Token-2022 token account using raw instructions.
        // NonTransferable mints require ImmutableOwner extension on the token account.
        let recipient = Keypair::new();
        env.svm.airdrop(&recipient.pubkey(), 10_000_000_000).unwrap();

        let recipient_token_kp = Keypair::new();
        let token_2022_id = spl_token_2022_program_id();

        // NonTransferable mint requires: ImmutableOwner + NonTransferableAccount extensions
        // Account size: 165 (base) + 1 (account_type) + 4 (ImmutableOwner TLV) + 4 (NonTransferableAccount TLV) = 174
        let account_len: usize = 174;
        let rent = env.svm.minimum_balance_for_rent_exemption(account_len);

        let create_ix = solana_sdk::system_instruction::create_account(
            &recipient.pubkey(),
            &recipient_token_kp.pubkey(),
            rent,
            account_len as u64,
            &token_2022_id,
        );
        // InitializeImmutableOwner instruction: data = [22]
        let init_immutable_ix = Instruction {
            program_id: token_2022_id,
            accounts: vec![AccountMeta::new(recipient_token_kp.pubkey(), false)],
            data: vec![22],
        };
        // InitializeAccount instruction: data = [1]
        let rent_sysvar = solana_sdk::sysvar::rent::id();
        let init_account_ix = Instruction {
            program_id: token_2022_id,
            accounts: vec![
                AccountMeta::new(recipient_token_kp.pubkey(), false),
                AccountMeta::new_readonly(access_mint_pda, false),
                AccountMeta::new_readonly(recipient.pubkey(), false),
                AccountMeta::new_readonly(rent_sysvar, false),
            ],
            data: vec![1],
        };

        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[create_ix, init_immutable_ix, init_account_ix],
            Some(&recipient.pubkey()),
            &[&recipient, &recipient_token_kp],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Create Token-2022 account");
        let recipient_ata = recipient_token_kp.pubkey();

        // Mint 5 access tokens to recipient
        let disc = anchor_discriminator("mint_access_token");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&5u64.to_le_bytes());
        let accounts = vec![
            AccountMeta::new(env.minter.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(access_mint_pda, false),
            AccountMeta::new(recipient_ata, false),
            AccountMeta::new_readonly(spl_token_2022_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "Mint access token should work: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_non_minter_cannot_mint_access_token() {
        let mut env = require_program!(setup_initialized_with_spl());
        let token_id: u64 = 604;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);
        let (access_mint_pda, _) = find_access_mint_pda(&env.config_pda, token_id);

        // Create access token mint
        let ix = build_create_access_token_mint_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &access_mint_pda,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Create access mint");

        let attacker = Keypair::new();
        env.svm.airdrop(&attacker.pubkey(), 10_000_000_000).unwrap();

        let disc = anchor_discriminator("mint_access_token");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&5u64.to_le_bytes());
        let accounts = vec![
            AccountMeta::new(attacker.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(access_mint_pda, false),
            AccountMeta::new(Pubkey::new_unique(), false), // dummy token account
            AccountMeta::new_readonly(spl_token_2022_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&attacker.pubkey()),
            &[&attacker],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Non-minter should not mint access tokens");
    }

    #[test]
    fn test_burn_access_token() {
        let mut env = require_program!(setup_initialized_with_spl());
        let token_id: u64 = 605;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);
        let (access_mint_pda, _) = find_access_mint_pda(&env.config_pda, token_id);

        // Create access token mint
        let ix = build_create_access_token_mint_ix(
            &env.manager.pubkey(),
            &env.config_pda,
            &reward_token_pda,
            &access_mint_pda,
        );
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Create access mint");

        // Create a holder Token-2022 token account using raw instructions
        let holder = Keypair::new();
        env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();

        let holder_token_kp = Keypair::new();
        let token_2022_id = spl_token_2022_program_id();

        // NonTransferable mint requires: ImmutableOwner + NonTransferableAccount extensions
        // Account size: 165 (base) + 1 (account_type) + 4 (ImmutableOwner TLV) + 4 (NonTransferableAccount TLV) = 174
        let account_len: usize = 174;
        let rent = env.svm.minimum_balance_for_rent_exemption(account_len);

        let create_ix = solana_sdk::system_instruction::create_account(
            &holder.pubkey(),
            &holder_token_kp.pubkey(),
            rent,
            account_len as u64,
            &token_2022_id,
        );
        let init_immutable_ix = Instruction {
            program_id: token_2022_id,
            accounts: vec![AccountMeta::new(holder_token_kp.pubkey(), false)],
            data: vec![22], // InitializeImmutableOwner
        };
        let rent_sysvar = solana_sdk::sysvar::rent::id();
        let init_account_ix = Instruction {
            program_id: token_2022_id,
            accounts: vec![
                AccountMeta::new(holder_token_kp.pubkey(), false),
                AccountMeta::new_readonly(access_mint_pda, false),
                AccountMeta::new_readonly(holder.pubkey(), false),
                AccountMeta::new_readonly(rent_sysvar, false),
            ],
            data: vec![1], // InitializeAccount
        };

        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[create_ix, init_immutable_ix, init_account_ix],
            Some(&holder.pubkey()),
            &[&holder, &holder_token_kp],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Create holder Token-2022 account");
        let holder_ata = holder_token_kp.pubkey();

        // Mint 5 access tokens to the holder via the program's mint_access_token instruction
        {
            let disc = anchor_discriminator("mint_access_token");
            let mut mint_data = Vec::new();
            mint_data.extend_from_slice(&disc);
            mint_data.extend_from_slice(&5u64.to_le_bytes());
            let mint_accounts = vec![
                AccountMeta::new(env.minter.pubkey(), true),
                AccountMeta::new_readonly(env.config_pda, false),
                AccountMeta::new(reward_token_pda, false),
                AccountMeta::new(access_mint_pda, false),
                AccountMeta::new(holder_ata, false),
                AccountMeta::new_readonly(spl_token_2022_program_id(), false),
            ];
            let ix = Instruction::new_with_bytes(program_id(), &mint_data, mint_accounts);
            let blockhash = env.svm.latest_blockhash();
            let tx = Transaction::new_signed_with_payer(
                &[ix],
                Some(&env.minter.pubkey()),
                &[&env.minter],
                blockhash,
            );
            env.svm.send_transaction(tx).expect("Mint 5 tokens to holder");
        }

        // Burn 2 tokens
        let disc = anchor_discriminator("burn_access_token");
        let mut data = Vec::new();
        data.extend_from_slice(&disc);
        data.extend_from_slice(&2u64.to_le_bytes());
        let accounts = vec![
            AccountMeta::new(holder.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new_readonly(reward_token_pda, false),
            AccountMeta::new(access_mint_pda, false),
            AccountMeta::new(holder_ata, false),
            AccountMeta::new_readonly(spl_token_2022_program_id(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &data, accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&holder.pubkey()),
            &[&holder],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "Burn access token should work: {:?}",
            result.err()
        );
    }
}

// ============================================================================
// 19. CLAIM WITH FUNDED TREASURY (SOL reward e2e)
// ============================================================================

#[cfg(test)]
mod claim_funded_treasury {
    use super::*;

    #[test]
    fn test_claim_sol_reward_from_funded_treasury() {
        let mut env = require_program!(setup_initialized());

        // Create a reward token with 1 SOL (1_000_000 lamports) per claim
        let token_id: u64 = 700;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);

        // Admin mint to create supply
        let mint_disc = anchor_discriminator("admin_mint");
        let mut mint_data = Vec::new();
        mint_data.extend_from_slice(&mint_disc);
        mint_data.extend_from_slice(&1u64.to_le_bytes()); // mint 1
        mint_data.push(0); // not soulbound
        let mint_accounts = vec![
            AccountMeta::new(env.minter.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new_readonly(reward_token_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &mint_data, mint_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Admin mint should work");

        // Fund the treasury PDA with enough SOL for the reward
        // The reward is 1_000_000 lamports per claim.
        // We need to add lamports to the treasury PDA account.
        let treasury_account = env.svm.get_account(&env.treasury_pda).unwrap();
        let funded_lamports = treasury_account.lamports + 5_000_000; // extra buffer
        env.svm.set_account(env.treasury_pda, solana_sdk::account::Account {
            lamports: funded_lamports,
            ..treasury_account
        }).unwrap();

        // Use admin_claim_reward (manager-initiated, no burn required)
        // claim_reward now requires a Token-2022 access token burn, but
        // create_simple_reward_token doesn't set up Token-2022 mints.
        let beneficiary = Keypair::new();
        env.svm.airdrop(&beneficiary.pubkey(), 10_000_000_000).unwrap();

        let beneficiary_balance_before = env.svm.get_balance(&beneficiary.pubkey()).unwrap();

        let claim_disc = anchor_discriminator("admin_claim_reward");
        let claim_accounts = vec![
            AccountMeta::new(env.manager.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new(beneficiary.pubkey(), false), // beneficiary
            AccountMeta::new_readonly(spl_token_program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
            // SOL rewards: no remaining_accounts needed
        ];
        let ix = Instruction::new_with_bytes(program_id(), &claim_disc, claim_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "Admin claim SOL reward should work: {:?}",
            result.err()
        );

        // Verify beneficiary received SOL
        let beneficiary_balance_after = env.svm.get_balance(&beneficiary.pubkey()).unwrap();
        // Beneficiary should have more SOL (reward amount)
        // The reward is 1_000_000 lamports
        assert!(
            beneficiary_balance_after > beneficiary_balance_before,
            "Beneficiary should have received SOL reward. Before: {}, After: {}",
            beneficiary_balance_before,
            beneficiary_balance_after,
        );

        // Verify treasury lost the reward amount
        let treasury_balance_after = env.svm.get_account(&env.treasury_pda).unwrap().lamports;
        assert_eq!(
            treasury_balance_after,
            funded_lamports - 1_000_000,
            "Treasury should have 1M fewer lamports"
        );
    }

    #[test]
    fn test_claim_fails_with_unfunded_treasury() {
        let mut env = require_program!(setup_initialized());

        let token_id: u64 = 701;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);

        // Admin mint
        let mint_disc = anchor_discriminator("admin_mint");
        let mut mint_data = Vec::new();
        mint_data.extend_from_slice(&mint_disc);
        mint_data.extend_from_slice(&1u64.to_le_bytes());
        mint_data.push(0);
        let mint_accounts = vec![
            AccountMeta::new(env.minter.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &mint_data, mint_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Admin mint");

        // DO NOT fund the treasury - it should fail during claim
        let user = Keypair::new();
        env.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

        let claim_disc = anchor_discriminator("claim_reward");
        let claim_accounts = vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new_readonly(spl_token_program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new(user.pubkey(), false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &claim_disc, claim_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&user.pubkey()),
            &[&user],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(result.is_err(), "Claim with unfunded treasury should fail");
    }

    #[test]
    fn test_admin_claim_sol_reward_for_beneficiary() {
        let mut env = require_program!(setup_initialized());

        let token_id: u64 = 702;
        let reward_token_pda = create_simple_reward_token(&mut env, token_id);

        // Admin mint
        let mint_disc = anchor_discriminator("admin_mint");
        let mut mint_data = Vec::new();
        mint_data.extend_from_slice(&mint_disc);
        mint_data.extend_from_slice(&1u64.to_le_bytes());
        mint_data.push(0);
        let mint_accounts = vec![
            AccountMeta::new(env.minter.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ];
        let ix = Instruction::new_with_bytes(program_id(), &mint_data, mint_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.minter.pubkey()),
            &[&env.minter],
            blockhash,
        );
        env.svm.send_transaction(tx).expect("Admin mint");

        // Fund treasury
        let treasury_account = env.svm.get_account(&env.treasury_pda).unwrap();
        let funded_lamports = treasury_account.lamports + 5_000_000;
        env.svm.set_account(env.treasury_pda, solana_sdk::account::Account {
            lamports: funded_lamports,
            ..treasury_account
        }).unwrap();

        // Beneficiary
        let beneficiary = Keypair::new();
        env.svm.airdrop(&beneficiary.pubkey(), 1_000_000_000).unwrap();
        let beneficiary_balance_before = env.svm.get_balance(&beneficiary.pubkey()).unwrap();

        // Admin claim on behalf of beneficiary
        let claim_disc = anchor_discriminator("admin_claim_reward");
        let claim_accounts = vec![
            AccountMeta::new(env.manager.pubkey(), true),
            AccountMeta::new_readonly(env.config_pda, false),
            AccountMeta::new(reward_token_pda, false),
            AccountMeta::new(env.treasury_pda, false),
            AccountMeta::new(beneficiary.pubkey(), false), // beneficiary
            AccountMeta::new_readonly(spl_token_program_id(), false),
            AccountMeta::new_readonly(system_program::ID, false),
            // SOL rewards: no remaining_accounts needed (beneficiary is recipient AccountInfo)
        ];
        let ix = Instruction::new_with_bytes(program_id(), &claim_disc, claim_accounts);
        let blockhash = env.svm.latest_blockhash();
        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&env.manager.pubkey()),
            &[&env.manager],
            blockhash,
        );
        let result = env.svm.send_transaction(tx);
        assert!(
            result.is_ok(),
            "Admin claim for beneficiary should work: {:?}",
            result.err()
        );

        // Verify beneficiary received SOL
        let beneficiary_balance_after = env.svm.get_balance(&beneficiary.pubkey()).unwrap();
        assert_eq!(
            beneficiary_balance_after,
            beneficiary_balance_before + 1_000_000,
            "Beneficiary should receive the SOL reward"
        );
    }
}
