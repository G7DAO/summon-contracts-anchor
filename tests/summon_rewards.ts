import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SummonRewards } from "../target/types/summon_rewards";

describe("summon_rewards", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.SummonRewards as Program<SummonRewards>;

  it("Is initialized!", async () => {
    // Add your test here.
  });
});
