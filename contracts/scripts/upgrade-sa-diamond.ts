import { ethers } from 'hardhat'
import {
    getFacets,
    getBytecodeFromFacet,
    getOnChainBytecodeFromFacets,
    upgradeFacetOnChain,
    upgradeFacet,
    logMissingFacetInfo,
} from './util'

/**
 * Upgrade the Subnet Actor Diamond.
 * @param deployments - The deployment data.
 * @returns An object of updated facets.
 */
async function upgradeSubnetActorDiamond(deployments) {
    const subnetActorDiamondAddress = deployments.SubnetActorDiamond

    const onChainFacets = await getFacets(subnetActorDiamondAddress)

    const updatedFacets = {}
    const onChainFacetBytecodes = await getOnChainBytecodeFromFacets(
        onChainFacets,
    )

    for (const facet of deployments.Facets) {
        await upgradeFacet(
            facet,
            onChainFacets,
            subnetActorDiamondAddress,
            updatedFacets,
            onChainFacetBytecodes,
            deployments,
        )
    }

    return updatedFacets
}

export { upgradeSubnetActorDiamond as upgradeDiamond }
