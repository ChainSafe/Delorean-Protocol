/* global ethers */

/* eslint prefer-const: "off" */
import { deployContractWithDeployer, getTransactionFees } from './util'
import { ethers } from 'hardhat'

export async function deploy() {
    await hre.run('compile')

    const [deployer] = await ethers.getSigners()
    const balance = await ethers.provider.getBalance(deployer.address)
    console.log(
        'Deploying libraries with the account:',
        deployer.address,
        ' balance:',
        ethers.utils.formatEther(balance),
    )

    const txArgs = await getTransactionFees()

    const { address: accountHelperAddress } = await deployContractWithDeployer(
        deployer,
        'AccountHelper',
        {},
        txArgs,
    )
    const { address: libStakingAddress } = await deployContractWithDeployer(
        deployer,
        'LibStaking',
        {},
        txArgs,
    )

    const { address: subnetIDHelperAddress } = await deployContractWithDeployer(
        deployer,
        'SubnetIDHelper',
        {},
        txArgs,
    )

    const { address: libQuorumAddress } = await deployContractWithDeployer(
        deployer,
        'LibQuorum',
        {},
        txArgs,
    )

    // nested libs
    const { address: crossMsgHelperAddress } = await deployContractWithDeployer(
        deployer,
        'CrossMsgHelper',
        { SubnetIDHelper: subnetIDHelperAddress },
        txArgs,
    )

    return {
        AccountHelper: accountHelperAddress,
        SubnetIDHelper: subnetIDHelperAddress,
        CrossMsgHelper: crossMsgHelperAddress,
        LibStaking: libStakingAddress,
        LibQuorum: libQuorumAddress,
    }
}

// deploy();
// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
if (require.main === module) {
    deploy()
        .then(() => process.exit(0))
        .catch((error: Error) => {
            console.error(error)
            process.exit(1)
        })
}
