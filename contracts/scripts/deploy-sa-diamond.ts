import { deployContractWithDeployer, getTransactionFees } from './util'
import hre, { ethers } from 'hardhat'

const { getSelectors, FacetCutAction } = require('./js/diamond.js')

async function deploySubnetActorDiamond(
    gatewayDiamondAddress: string,
    libs: { [key in string]: string },
) {
    if (!gatewayDiamondAddress) throw new Error(`Gateway is missing`)
    if (!libs || Object.keys(libs).length === 0)
        throw new Error(`Libraries are missing`)

    console.log('Deploying Subnet Actor diamond with libraries:', libs)

    await hre.run('compile')

    const [deployer] = await ethers.getSigners()
    const txArgs = await getTransactionFees()

    type Libraries = {
        [libraryName: string]: string
    }

    const getterFacetLibs: Libraries = {
        SubnetIDHelper: libs['SubnetIDHelper'],
    }

    const managerFacetLibs: Libraries = {}

    const rewarderFacetLibs: Libraries = {}

    const pauserFacetLibs: Libraries = {}

    const checkpointerFacetLibs: Libraries = {}

    const facets = [
        { name: 'DiamondLoupeFacet', libs: {} },
        { name: 'DiamondCutFacet', libs: {} },
        { name: 'SubnetActorGetterFacet', libs: getterFacetLibs },
        { name: 'SubnetActorManagerFacet', libs: managerFacetLibs },
        { name: 'SubnetActorRewardFacet', libs: rewarderFacetLibs },
        { name: 'SubnetActorCheckpointingFacet', libs: checkpointerFacetLibs },
        { name: 'SubnetActorPauseFacet', libs: pauserFacetLibs },
        { name: 'OwnershipFacet', libs: {} },
    ]
    // The `facetCuts` variable is the FacetCut[] that contains the functions to add during diamond deployment
    const facetCuts = []

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

    const gatewayGetterFacet = await ethers.getContractAt(
        'GatewayGetterFacet',
        gatewayDiamondAddress,
    )
    const parentId = await gatewayGetterFacet.getNetworkName()
    console.log('parentId', parentId[0])
    console.log('parentId', parentId[1])

    const constructorParams = {
        parentId,
        ipcGatewayAddr: gatewayDiamondAddress,
        consensus: 0,
        minActivationCollateral: ethers.utils.parseEther('1'),
        minValidators: 3,
        bottomUpCheckPeriod: 10,
        majorityPercentage: 66,
        activeValidatorsLimit: 100,
        minCrossMsgFee: 1,
        powerScale: 1,
    }

    console.log('constructorParams', constructorParams)

    const diamondLibs: Libraries = {
        SubnetIDHelper: libs['SubnetIDHelper'],
    }

    // deploy Diamond
    const { address: diamondAddress } = await deployContractWithDeployer(
        deployer,
        'SubnetActorDiamond',
        diamondLibs,
        facetCuts,
        constructorParams,
        txArgs,
    )

    console.log('Subnet Actor Diamond address:', diamondAddress)

    // returning the address of the diamond
    return {
        SubnetActorDiamond: diamondAddress,
        Facets: facets,
    }
}

// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
if (require.main === module) {
    deploySubnetActorDiamond()
        .then(() => process.exit(0))
        .catch((error) => {
            console.error(error)
            process.exit(1)
        })
}

exports.deployDiamond = deploySubnetActorDiamond
