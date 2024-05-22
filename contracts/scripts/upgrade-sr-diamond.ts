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
 * Upgrade the Subnet Registry Diamond.
 * @param deployments - The deployment data.
 * @returns An object of updated facets.
 */
async function upgradeSubnetRegistryDiamond(deployments) {
    const subnetRegistryDiamondAddress = deployments.SubnetRegistry

    const onChainFacets = await getFacets(subnetRegistryDiamondAddress)
    const updatedFacets = {}
    const onChainFacetBytecodes = await getOnChainBytecodeFromFacets(
        onChainFacets,
    )

    for (const facet of deployments.Facets) {
        await upgradeFacet(
            facet,
            onChainFacets,
            subnetRegistryDiamondAddress,
            updatedFacets,
            onChainFacetBytecodes,
            deployments,
        )
    }

    return updatedFacets
}

export { upgradeSubnetRegistryDiamond as upgradeDiamond }
