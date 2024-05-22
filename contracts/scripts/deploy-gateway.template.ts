/* global ethers */

/* eslint prefer-const: "off" */
import { deployContractWithDeployer, getTransactionFees } from './util'
import { ethers } from 'hardhat'

const { getSelectors, FacetCutAction } = require('./js/diamond.js')

function getGitCommitSha(): string {
    const commitSha = require('child_process')
        .execSync('git rev-parse --short HEAD')
        .toString()
        .trim()
    return commitSha
}
export async function deploy(libs: { [key in string]: string }) {
    if (!libs || Object.keys(libs).length === 0)
        throw new Error(`Libraries are missing`)

    // choose chain ID according to the network in
    // environmental variable
    let chainId = 31415926
    if (process.env.NETWORK == 'calibrationnet') {
        chainId = 314159
    } else if (process.env.NETWORK == 'mainnet') {
        chainId = 314
    } else if (process.env.NETWORK == 'auto') {
        chainId = parseInt(process.env.CHAIN_ID!, 16)
    }

    await hre.run('compile')

    const [deployer] = await ethers.getSigners()
    const balance = await ethers.provider.getBalance(deployer.address)
    console.log(
        'Deploying gateway with the account:',
        deployer.address,
        ' balance:',
        ethers.utils.formatEther(balance),
    )

    const txArgs = await getTransactionFees()

    const facetCuts = []

    type Libraries = {
        [libraryName: string]: string
    }

    const getterFacetLibs: Libraries = {
        SubnetIDHelper: libs['SubnetIDHelper'],
        LibQuorum: libs['LibQuorum'],
    }

    const managerFacetLibs: Libraries = {
        CrossMsgHelper: libs['CrossMsgHelper'],
        SubnetIDHelper: libs['SubnetIDHelper'],
    }
    const messengerFacetLibs: Libraries = {
        SubnetIDHelper: libs['SubnetIDHelper'],
        CrossMsgHelper: libs['CrossMsgHelper'],
    }

    const checkpointingFacetLibs: Libraries = {
        AccountHelper: libs['AccountHelper'],
        SubnetIDHelper: libs['SubnetIDHelper'],
        CrossMsgHelper: libs['CrossMsgHelper'],
    }

    const xnetMessagingFacetLibs: Libraries = {
        AccountHelper: libs['AccountHelper'],
        CrossMsgHelper: libs['CrossMsgHelper'],
        SubnetIDHelper: libs['SubnetIDHelper'],
    }

    const topDownFinalityFacetLibs: Libraries = {
        AccountHelper: libs['AccountHelper'],
    }

    const facets = [
        { name: 'GatewayGetterFacet', libs: getterFacetLibs },
        { name: 'DiamondLoupeFacet', libs: {} },
        { name: 'DiamondCutFacet', libs: {} },
        { name: 'GatewayManagerFacet', libs: managerFacetLibs },
        { name: 'GatewayMessengerFacet', libs: messengerFacetLibs },
        {
            name: 'CheckpointingFacet',
            libs: checkpointingFacetLibs,
        },
        {
            name: 'XnetMessagingFacet',
            libs: xnetMessagingFacetLibs,
        },
        { name: 'TopDownFinalityFacet', libs: topDownFinalityFacetLibs },
        { name: 'OwnershipFacet', libs: {} },
    ]

    for (const facet of facets) {
        const facetInstance = await deployContractWithDeployer(
            deployer,
            facet.name,
            facet.libs,
            txArgs,
        )
        await facetInstance.deployed()

        facet.address = facetInstance.address

        facetCuts.push({
            facetAddress: facetInstance.address,
            action: FacetCutAction.Add,
            functionSelectors: getSelectors(facetInstance),
        })
    }

    const gatewayConstructorParams = {
        bottomUpCheckPeriod: 10,
        activeValidatorsLimit: 100,
        majorityPercentage: 66,
        networkName: {
            root: chainId,
            route: [],
        },
        genesisValidators: [],
        commitSha: ethers.utils.formatBytes32String(getGitCommitSha()),
    }

    const diamondLibs: Libraries = {}
    // deploy Diamond
    const { address: gatewayAddress } = await deployContractWithDeployer(
        deployer,
        'GatewayDiamond',
        diamondLibs,
        facetCuts,
        gatewayConstructorParams,
        txArgs,
    )

    // returning the address of the diamond
    return {
        ChainID: chainId,
        Gateway: gatewayAddress,
        Facets: facets,
    }
}
