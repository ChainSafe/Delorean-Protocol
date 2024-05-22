import { SignerWithAddress } from '@nomiclabs/hardhat-ethers/signers'
import { providers, Wallet, ContractFactory, Contract } from 'ethers'
import ganache from 'ganache'
import { ethers } from 'hardhat'
import * as linker from 'solc/linker'

const { getSelectors, FacetCutAction } = require('./js/diamond.js')
const fs = require('fs')
const path = require('path')

function findFileInDir(filename, folder) {
    // This function checks each entry in the directory: if it's a file matching the filename, it returns its path.
    // If it's a directory, the function recurses into it.
    function searchDirectory(currentPath) {
        const entries = fs.readdirSync(currentPath, { withFileTypes: true })

        for (let entry of entries) {
            const entryPath = path.join(currentPath, entry.name)

            if (entry.isDirectory()) {
                const result = searchDirectory(entryPath)
                if (result) return result
            } else if (entry.isFile() && entry.name === filename) {
                return entryPath
            }
        }

        // If no file is found, return null.
        return null
    }

    return searchDirectory(folder)
}

export const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000'

const isolatedPort = 18678

export enum SubnetCreationPrivileges {
    Unrestricted = 0,
    Owner = 1,
}

export async function deployContractWithDeployer(
    deployer: SignerWithAddress,
    contractName: string,
    libs: { [key in string]: string },
    ...args: any[]
): Promise<Contract> {
    const contractFactory = await ethers.getContractFactory(contractName, {
        signer: deployer,
        libraries: libs,
    })
    return contractFactory.deploy(...args)
}

export function subnetCreationPrivileges(): SubnetCreationPrivileges {
    const value = process.env.REGISTRY_CREATION_PRIVILEGES || 'unrestricted'
    return value === 'owner'
        ? SubnetCreationPrivileges.Owner
        : SubnetCreationPrivileges.Unrestricted
}

export async function getTransactionFees() {
    const feeData = await ethers.provider.getFeeData()

    return {
        maxFeePerGas: feeData.maxFeePerGas,
        maxPriorityFeePerGas: feeData.maxPriorityFeePerGas,
        type: 2,
    }
}

interface Facet {
    facetAddress: string
    functionSelectors: string[]
}
type FacetMap = { [key: string]: string[] }

export async function getFacets(diamondAddress: string): Promise<FacetMap> {
    // Ensure you have the ABI for the diamond loupe functions
    const diamondLoupeABI = [
        {
            inputs: [],
            name: 'facets',
            outputs: [
                {
                    components: [
                        {
                            internaltype: 'address',
                            name: 'facetaddress',
                            type: 'address',
                        },
                        {
                            internaltype: 'bytes4[]',
                            name: 'functionselectors',
                            type: 'bytes4[]',
                        },
                    ],
                    name: 'facets_',
                    type: 'tuple[]',
                },
            ],
            statemutability: 'view',
            constant: true,
            type: 'function',
        },
    ]

    const provider = ethers.provider
    const diamond = new Contract(diamondAddress, diamondLoupeABI, provider)
    const facetsData = await diamond.facets()

    // Convert facetsData to the Facet[] type.
    const facets: Facet[] = facetsData.map((facetData) => ({
        facetAddress: facetData[0],
        functionSelectors: facetData[1],
    }))

    const facetMap = facets.reduce((acc, facet) => {
        acc[facet.facetAddress] = facet.functionSelectors
        return acc
    }, {})

    return facetMap
}

async function startGanache() {
    return new Promise((resolve, reject) => {
        const server = ganache.server({
            miner: { defaultGasPrice: '0x0' },
            chain: { hardfork: 'berlin' },
            logging: { quiet: true },
        })
        server.listen(isolatedPort, (err) => {
            if (err) reject(err)
            else resolve(server)
        })
    })
}

async function stopGanache(server) {
    return new Promise((resolve, reject) => {
        server.close((err) => {
            if (err) reject(err)
            else resolve()
        })
    })
}

export async function getRuntimeBytecode(bytecode) {
    // Check if bytecode is provided
    if (!bytecode) {
        throw new Error('No bytecode provided')
    }
    const ganacheServer = await startGanache()

    const provider = new providers.JsonRpcProvider(
        `http://127.0.0.1:${isolatedPort}`,
    )
    const wallet = new Wallet(process.env.PRIVATE_KEY, provider)
    const contractFactory = new ContractFactory([], bytecode, wallet)
    const contract = await contractFactory.deploy({ gasPrice: 0 })
    await contract.deployed()

    const runtimeBytecode = await provider.getCode(contract.address)
    stopGanache(ganacheServer)
    return runtimeBytecode
}

export async function getBytecodeFromFacet(facet) {
    const facetName = facet.name
    const libs = facet.libs
    const factoryFileName = findFileInDir(
        `${facetName}__factory.ts`,
        `./typechain/factories/`,
    )
    if (factoryFileName === null) {
        throw new Error('Typescript bindings for Facet not found')
    }
    const bytecodeNeedsLink =
        getBytecodeFromFacetTypeChainFilename(factoryFileName)
    let libs2 = {}
    // Loop through each key in the libs
    for (let key in libs) {
        let newKey = `src/lib/${key}.sol:${key}`
        libs2[newKey] = libs[key]
    }

    // Link the bytecode with the libraries
    const bytecode = linker.linkBytecode(bytecodeNeedsLink, libs2)
    return await getRuntimeBytecode(bytecode)
}

function getBytecodeFromFacetTypeChainFilename(fileName) {
    try {
        // Read the file synchronously
        const fileContent = fs.readFileSync(fileName, 'utf8')

        // Split the file content into lines
        const lines = fileContent.split('\n')

        // Initialize a flag to identify when the target line is found
        let found = false

        for (const line of lines) {
            // If the previous line was the target line, return the current line
            if (found) {
                // Trim semicolons and quotes from the beginning and end of the string
                return line.trim().replace(/^[";]+|[";]+$/g, '')
            }

            // Check if the current line is the target line
            if (line.includes('const _bytecode =')) {
                found = true
            }
        }

        // If the loop completes without returning, the target line was not found
        throw new Error('Target line "const _bytecode =" not found in the file')
    } catch (error) {
        console.error('Error reading file:', error.message)
    }
}

// Loop through each contract address in the facets
// query web3 api to get deployed bytecode
export async function getOnChainBytecodeFromFacets(facets) {
    const deployedBytecode = {}
    for (let contractAddress in facets) {
        try {
            // Fetch the bytecode of the contract
            const bytecode = await ethers.provider.getCode(contractAddress)
            deployedBytecode[bytecode] = contractAddress
            // Log the bytecode to the console
        } catch (error) {
            // Print any errors to stderr
            console.error(
                `Error fetching bytecode for ${contractAddress}:`,
                error.message,
            )
        }
    }
    return deployedBytecode
}

/**
 * Filters the input array to only return strings that start with '0x'.
 *
 * @param {Object} input - The object containing the functionSelectors array.
 * @returns {Array} - An array of strings from functionSelectors that start with '0x'.
 */
function filterSelectors(input) {
    return input.filter((item) => {
        return typeof item === 'string' && item.startsWith('0x')
    })
}

function compareArrays(onChain, newArr) {
    const result = {
        removedSelectors: [],
        matchingSelectors: [],
        addedSelectors: [],
    }

    // Create a Map for easier lookup
    const onChainMap = new Map()
    onChain.forEach((selector) => onChainMap.set(selector, true))

    const newArrMap = new Map()
    newArr.forEach((selector) => newArrMap.set(selector, true))

    // Find matching and removed selectors
    onChain.forEach((selector) => {
        if (newArrMap.has(selector)) {
            result.matchingSelectors.push(selector)
        } else {
            result.removedSelectors.push(selector)
        }
    })

    // Find added selectors
    newArr.forEach((selector) => {
        if (!onChainMap.has(selector)) {
            result.addedSelectors.push(selector)
        }
    })

    return result
}
async function cutFacetOnChain(
    diamondAddress: string,
    replacementFacet: any,
    action,
    functionSelectors,
) {
    const [deployer] = await ethers.getSigners()
    const txArgs = await getTransactionFees()

    const facetCuts = [
        {
            facetAddress:
                action === FacetCutAction.Remove
                    ? ethers.constants.AddressZero
                    : replacementFacet.address,
            action: action,
            functionSelectors: functionSelectors,
        },
    ]
    const diamondCutter = await ethers.getContractAt(
        'DiamondCutFacet',
        diamondAddress,
        deployer,
    )
    const tx = await diamondCutter.diamondCut(
        facetCuts,
        ethers.constants.AddressZero,
        ethers.utils.formatBytes32String(''),
        txArgs,
    )
    await tx.wait()
}

// given a facet address and a diamond address,
// upgrade the diamond to use the new facet
export async function upgradeFacetOnChain(
    diamondAddress: string,
    facet,
    onChainFunctionSelectors,
) {
    const replacementFacetName = facet.name
    const facetLibs = facet.libs
    console.info(`
Diamond Facet Upgrade:
-----------------------------------
Diamond Address: ${diamondAddress}
Replacement Facet Name: ${replacementFacetName}
`)

    if (!diamondAddress) throw new Error(`Gateway is missing`)

    const [deployer] = await ethers.getSigners()
    const txArgs = await getTransactionFees()
    let replacementFacet = await deployContractWithDeployer(
        deployer,
        replacementFacetName,
        facetLibs,
        txArgs,
    )
    await replacementFacet.deployed()

    const result = compareArrays(
        onChainFunctionSelectors,
        filterSelectors(getSelectors(replacementFacet)),
    )

    async function cutSelectorsOnChain(action, selectors) {
        if (selectors.length > 0) {
            await cutFacetOnChain(
                diamondAddress,
                replacementFacet,
                action,
                selectors,
            )
        }
    }

    // cut changes for each facet cut action - remove replace and add
    await cutSelectorsOnChain(FacetCutAction.Remove, result['removedSelectors'])
    await cutSelectorsOnChain(
        FacetCutAction.Replace,
        result['matchingSelectors'],
    )
    await cutSelectorsOnChain(FacetCutAction.Add, result['addedSelectors'])

    //end move facet
    return { [replacementFacetName]: replacementFacet.address }
}

/**
 * Log information about a missing facet.
 * @param facet - The facet to display.
 */
export function logMissingFacetInfo(facet) {
    const formattedLibs = Object.entries(facet.libs)
        .map(([key, value]) => `  - ${key}: ${value}`)
        .join('\n')

    console.info(`
Facet Bytecode Not Found:
---------------------------------
Facet Name: ${facet.name}
Libraries:
${formattedLibs}
Address: ${facet.address}
`)
}

function getDeployedFacetAddressFromName(facetName, deployments) {
    for (let facet of deployments.Facets) {
        if (facet.name === facetName) {
            return facet.address
        }
    }
}

/**
 * Handle facet upgrades on chain.
 * @param facet - The facet to process.
 * @param onChainFacets - the on chain facets and their functions as returned by DiamondLoupe
 * @param gatewayDiamondAddress - The address of the Gateway Diamond.
 * @param updatedFacets - A collection of updated facets.
 * @param onChainFacetBytecodes - The bytecodes from the on-chain facets.
 */
export async function upgradeFacet(
    facet,
    onChainFacets,
    gatewayDiamondAddress,
    updatedFacets,
    onChainFacetBytecodes,
    deployments,
) {
    const facetBytecode = await getBytecodeFromFacet(facet)

    if (!onChainFacetBytecodes[facetBytecode]) {
        logMissingFacetInfo(facet)

        const onChainFunctionSelectors =
            onChainFacets[
                getDeployedFacetAddressFromName(facet.name, deployments)
            ]

        const newFacet = await upgradeFacetOnChain(
            gatewayDiamondAddress,
            facet,
            onChainFunctionSelectors,
        )
        for (let key in newFacet) updatedFacets[key] = newFacet[key]

        const DEPLOYMENT_STATUS_MESSAGE = `
Deployment Status:
-------------------------
New replacement facet (${facet.name}) deployed.
`
        console.info(DEPLOYMENT_STATUS_MESSAGE)
    }
}
