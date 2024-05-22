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
 * Upgrade the Gateway Actor Diamond.
 * @param deployments - The deployment data.
 * @returns An object of updated facets.
 */
async function upgradeGatewayActorDiamond(deployments) {
    const gatewayDiamondAddress = deployments.Gateway

    const onChainFacets = await getFacets(gatewayDiamondAddress)
    const updatedFacets = {}
    const onChainFacetBytecodes = await getOnChainBytecodeFromFacets(
        onChainFacets,
    )

    for (const facet of deployments.Facets) {
        await upgradeFacet(
            facet,
            onChainFacets,
            gatewayDiamondAddress,
            updatedFacets,
            onChainFacetBytecodes,
            deployments,
        )
    }

    return updatedFacets
}

export { upgradeGatewayActorDiamond as upgradeDiamond }
