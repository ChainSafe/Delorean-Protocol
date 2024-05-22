import {
    deployContractWithDeployer,
    getTransactionFees,
    subnetCreationPrivileges,
} from './util'
import { ethers } from 'hardhat'

const { getSelectors, FacetCutAction } = require('./js/diamond.js')

export async function deploy() {
    const [deployer] = await ethers.getSigners()
    const balance = await deployer.getBalance()

    console.log(
        `Deploying contracts with account: ${
            deployer.address
        } and balance: ${balance.toString()}`,
    )

    const mode = subnetCreationPrivileges()
    console.log(
        `
            ***************************************************************
            **                                                           **
            **  Subnet creation privileges: ${mode}                      **
            **                                                           **
            ***************************************************************
        `,
    )

    const gatewayAddress = GATEWAY.Gateway
    const txArgs = await getTransactionFees()

    const subnetActorDeployFacets = []

    // deploy
    const getterFacet = await deployContractWithDeployer(
        deployer,
        'SubnetActorGetterFacet',
        {
            SubnetIDHelper: LIBMAP['SubnetIDHelper'],
        },
        txArgs,
    )
    const getterSelectors = getSelectors(getterFacet)

    subnetActorDeployFacets.push({
        name: 'SubnetActorGetterFacet',
        libs: {
            SubnetIDHelper: LIBMAP['SubnetIDHelper'],
        },
        address: getterFacet.address,
    })

    const managerFacet = await deployContractWithDeployer(
        deployer,
        'SubnetActorManagerFacet',
        {},
        txArgs,
    )
    const managerSelectors = getSelectors(managerFacet)

    subnetActorDeployFacets.push({
        name: 'SubnetActorManagerFacet',
        libs: {},
        address: managerFacet.address,
    })

    const pauserFacet = await deployContractWithDeployer(
        deployer,
        'SubnetActorPauseFacet',
        {},
        txArgs,
    )
    const pauserSelectors = getSelectors(pauserFacet)

    subnetActorDeployFacets.push({
        name: 'SubnetActorPauseFacet',
        libs: {},
        address: pauserFacet.address,
    })

    const rewarderFacet = await deployContractWithDeployer(
        deployer,
        'SubnetActorRewardFacet',
        {},
        txArgs,
    )
    const rewarderSelectors = getSelectors(rewarderFacet)
    subnetActorDeployFacets.push({
        name: 'SubnetActorRewardFacet',
        libs: {},
        address: rewarderFacet.address,
    })

    const checkpointerFacet = await deployContractWithDeployer(
        deployer,
        'SubnetActorCheckpointingFacet',
        {},
        txArgs,
    )
    const checkpointerSelectors = getSelectors(checkpointerFacet)
    subnetActorDeployFacets.push({
        name: 'SubnetActorCheckpointingFacet',
        libs: {},
        address: checkpointerFacet.address,
    })

    const diamondCutFacet = await deployContractWithDeployer(
        deployer,
        'DiamondCutFacet',
        {},
        txArgs,
    )
    const diamondCutSelectors = getSelectors(diamondCutFacet)
    subnetActorDeployFacets.push({
        name: 'DiamondCutFacet',
        libs: {},
        address: diamondCutFacet.address,
    })

    const diamondLoupeFacet = await deployContractWithDeployer(
        deployer,
        'DiamondLoupeFacet',
        {},
        txArgs,
    )
    const diamondLoupeSelectors = getSelectors(diamondLoupeFacet)
    subnetActorDeployFacets.push({
        name: 'DiamondLoupeFacet',
        libs: {},
        address: diamondLoupeFacet.address,
    })

    const ownershipFacet = await deployContractWithDeployer(
        deployer,
        'OwnershipFacet',
        {},
        txArgs,
    )
    const ownershipSelectors = getSelectors(ownershipFacet)
    subnetActorDeployFacets.push({
        name: 'OwnershipFacet',
        libs: {},
        address: ownershipFacet.address,
    })

    //deploy subnet registry diamond
    const registry = await ethers.getContractFactory('SubnetRegistryDiamond', {
        signer: deployer,
    })

    const registryConstructorParams = {
        gateway: gatewayAddress,
        getterFacet: getterFacet.address,
        managerFacet: managerFacet.address,
        rewarderFacet: rewarderFacet.address,
        checkpointerFacet: checkpointerFacet.address,
        pauserFacet: pauserFacet.address,
        diamondCutFacet: diamondCutFacet.address,
        diamondLoupeFacet: diamondLoupeFacet.address,
        ownershipFacet: ownershipFacet.address,
        subnetActorGetterSelectors: getterSelectors,
        subnetActorManagerSelectors: managerSelectors,
        subnetActorRewarderSelectors: rewarderSelectors,
        subnetActorCheckpointerSelectors: checkpointerSelectors,
        subnetActorPauserSelectors: pauserSelectors,
        subnetActorDiamondCutSelectors: diamondCutSelectors,
        subnetActorDiamondLoupeSelectors: diamondLoupeSelectors,
        subnetActorOwnershipSelectors: ownershipSelectors,
        creationPrivileges: Number(mode),
    }

    const facetCuts = [] //TODO

    const facets = [
        {
            name: 'RegisterSubnetFacet',
            libs: {
                SubnetIDHelper: LIBMAP['SubnetIDHelper'],
            },
        },
        { name: 'SubnetGetterFacet', libs: {} },
        { name: 'DiamondLoupeFacet', libs: {} },
        { name: 'DiamondCutFacet', libs: {} },
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

    const diamondLibs = {
        SubnetIDHelper: LIBMAP['SubnetIDHelper'],
    }
    // deploy Diamond
    const { address: subnetRegistryAddress } = await deployContractWithDeployer(
        deployer,
        'SubnetRegistryDiamond',
        {},
        facetCuts,
        registryConstructorParams,
        txArgs,
    )

    return {
        SubnetRegistry: subnetRegistryAddress,
        Facets: facets,
        SubnetActorFacets: subnetActorDeployFacets,
    }
}
