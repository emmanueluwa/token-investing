import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { SYSTEM_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/native/system";
import { PublicKey, Keypair } from "@solana/web3.js";
import {
  BanksClient,
  ProgramTestContext,
  Clock,
  startAnchor,
} from "solana-bankrun";
import { BankrunProvider } from "anchor-bankrun";
import { createMint, mintTo } from "spl-token-bankrun";

import { Tokenvesting } from "../target/types/tokenvesting";
import IDL from "../target/idl/tokenvesting.json";
import NodeWallet from "@coral-xyz/anchor/dist/cjs/nodewallet";
import { TOKEN_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/utils/token";

//bankrun test setup has wallet by default, funded and ready
describe("Vesting Smart Contract Tests", () => {
  const companyName = "companyName";

  let beneficiary: Keypair;
  let context: ProgramTestContext;
  let provider: BankrunProvider;
  let program: Program<Tokenvesting>;
  let banksClient: BanksClient;
  let employer: Keypair;
  let mint: PublicKey;
  let beneficiaryProvider: BankrunProvider;
  let program2: Program<Tokenvesting>;
  let vestingAccountKey: PublicKey;
  let treasuryTokenAccount: PublicKey;
  let employeeAccount: PublicKey;

  beforeAll(async () => {
    //new wallet for beneficiary
    beneficiary = new anchor.web3.Keypair();

    context = await startAnchor(
      "", //path to route of anchor program
      [{ name: "tokenvesting", programId: new PublicKey(IDL.address) }],
      [
        {
          address: beneficiary.publicKey,
          info: {
            lamports: 1_000_000_000,
            data: Buffer.alloc(0),
            owner: SYSTEM_PROGRAM_ID,
            executable: false,
          },
        },
      ]
    );

    //wallet for employer
    provider = new BankrunProvider(context);
    anchor.setProvider(provider);

    program = new Program<Tokenvesting>(IDL as Tokenvesting, provider);

    banksClient = context.banksClient;

    employer = provider.wallet.payer;

    // spl token mint passed between treasury and employee accounts
    // @ts-ignore
    mint = await createMint(banksClient, employer, employer.publicKey, null, 2);

    beneficiaryProvider = new BankrunProvider(context);
    beneficiaryProvider.wallet = new NodeWallet(beneficiary);

    program2 = new Program<Tokenvesting>(
      IDL as Tokenvesting,
      beneficiaryProvider
    );

    // derive pda's
    // - vesting account key
    [vestingAccountKey] = PublicKey.findProgramAddressSync(
      [Buffer.from(companyName)],
      program.programId
    );

    //treasury token account key
    [treasuryTokenAccount] = PublicKey.findProgramAddressSync(
      [Buffer.from("vesting_treasury"), Buffer.from(companyName)],
      program.programId
    );

    //employee account
    [employeeAccount] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("employee_vesting"),
        beneficiary.publicKey.toBuffer(),
        vestingAccountKey.toBuffer(),
      ],
      program.programId
    );
  });

  it("should create a vesting account", async () => {
    const tx = await program.methods
      .createVestingAccount(companyName)
      .accounts({
        signer: employer.publicKey,
        mint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc({ commitment: "confirmed" });

    const vestingAccountData = await program.account.vestingAccount.fetch(
      vestingAccountKey,
      "confirmed"
    );

    console.log("vesting account data", vestingAccountData, null, 2);
    console.log("create vesting acc: ", tx);
  });

  it("should fund treasury token account", async () => {
    const amount = 10_000 * 10 ** 9;
    const mintTx = await mintTo(
      banksClient,
      employer,
      mint,
      treasuryTokenAccount,
      employer,
      amount
    );

    console.log("Mint treasury token account", mintTx);
  });

  it("should create employee vesting account", async () => {
    const tx2 = await program.methods
      .createEmployeeAccount(new BN(0), new BN(100), new BN(100), new BN(0))
      .accounts({
        beneficiary: beneficiary.publicKey,
        vestingAccount: vestingAccountKey,
      })
      .rpc({ commitment: "confirmed", skipPreflight: true });

    console.log("Create employee account tx: ", tx2);
    console.log("employee account: ", employeeAccount.toBase58());
  });

  it("should claim the employee's vested tokens", async () => {
    await new Promise((resolve) => setTimeout(resolve, 1000));

    const currentClock = await banksClient.getClock();
    context.setClock(
      new Clock(
        currentClock.slot,
        currentClock.epochStartTimestamp,
        currentClock.epoch,
        currentClock.leaderScheduleEpoch,
        1000n
      )
    );

    //signer is beneficiary not employer so use program2
    const tx3 = await program2.methods
      .claimTokens(companyName)
      .accounts({ tokenProgram: TOKEN_PROGRAM_ID })
      .rpc({ commitment: "confirmed" });

    console.log("claim tokens tx:", tx3);
  });
});
