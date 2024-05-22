import argparse
import glob
import json
import os
import subprocess
import sys
from eth_abi import encode
from json.decoder import JSONDecodeError

def writeToFile(selector_storage_content):
    # Define the file path
    file_path = 'test/helpers/SelectorLibrary.sol'

    # Write the content to the file
    with open(file_path, 'w') as file:
        file.write(selector_storage_content)


def generate_solidity_function(contract_selectors):
    solidity_code = "// SPDX-License-Identifier: MIT OR Apache-2.0\npragma solidity ^0.8.19;\n"
    solidity_code += "library SelectorLibrary {\n"
    solidity_code += "    function resolveSelectors(string memory facetName) public pure returns (bytes4[] memory facetSelectors) {\n"

    for contract_name, selectors in contract_selectors.items():
        solidity_code += f'        if (keccak256(abi.encodePacked(facetName)) == keccak256(abi.encodePacked("{contract_name}"))) {{\n'
        solidity_code += f'            return abi.decode(hex"{selectors}", (bytes4[]));\n'
        solidity_code += "        }\n"

    solidity_code += "        revert(\"Selector not found\");\n"
    solidity_code += "    }\n"
    solidity_code += "}\n"
    return solidity_code

def format_selector(selector_bytes):
    hex_str = selector_bytes.hex()
    if len(hex_str) % 2 != 0:
        hex_str = '0' + hex_str  # Add a leading zero if the length is odd
    return hex_str

def parse_selectors(encoded_selectors):
    # Assuming the encoded selectors are in the format provided in your example
    decoded = bytes.fromhex(encoded_selectors[2:])  # Skip the "0x" prefix
    return [format_selector(decoded[i:i+4]) for i in range(0, len(decoded), 4)]  # Return in chunks of 4 bytes

def get_selectors(contract):
    """This function gets the selectors of the functions of the target contract."""

    res = subprocess.run(
        ["forge", "inspect", contract, "methodIdentifiers"], capture_output=True)
    res = res.stdout.decode()
    try:
        res = json.loads(res)
    except JSONDecodeError as e:
        print("failed to load JSON:", e)
        print("forge output:", res)
        print("contract:", contract)
        sys.exit(1)

    selectors = []
    for signature in res:
        selector = res[signature]
        selectors.append(bytes.fromhex(selector))

    enc = encode(["bytes4[]"], [selectors])
    return "" + enc.hex()

def main():
    contract_selectors = {}
    filepaths_to_target = [
         'src/GatewayDiamond.sol',
         'src/SubnetActorDiamond.sol',
         'src/SubnetRegistryDiamond.sol',
         'src/OwnershipFacet.sol',
         'src/diamond/DiamondCutFacet.sol',
         'src/diamond/DiamondLoupeFacet.sol',
         'src/gateway/GatewayGetterFacet.sol',
         'src/gateway/GatewayManagerFacet.sol',
         'src/gateway/GatewayMessengerFacet.sol',
         'src/gateway/router/CheckpointingFacet.sol',
         'src/gateway/router/TopDownFinalityFacet.sol',
         'src/gateway/router/XnetMessagingFacet.sol',
         'src/subnet/SubnetActorGetterFacet.sol',
         'src/subnet/SubnetActorManagerFacet.sol',
         'src/subnet/SubnetActorPauseFacet.sol',
         'src/subnet/SubnetActorRewardFacet.sol',
         'src/subnet/SubnetActorCheckpointingFacet.sol',
         'src/subnetregistry/RegisterSubnetFacet.sol',
         'src/subnetregistry/SubnetGetterFacet.sol',
         'test/helpers/ERC20PresetFixedSupply.sol',
         'test/helpers/NumberContractFacetEight.sol',
         'test/helpers/NumberContractFacetSeven.sol',
         'test/helpers/SelectorLibrary.sol',
         'test/helpers/TestUtils.sol',
         'test/mocks/SubnetActorMock.sol',
     ]

    for filepath in filepaths_to_target:

        # Extract just the contract name (without path and .sol extension)
        contract_name = os.path.splitext(os.path.basename(filepath))[0]

        #skip lib or interfaces
        if contract_name.startswith("Lib") or contract_name.startswith("I") or contract_name.endswith("Helper"):
            continue

        # Format full path
        # Call get_selectors for each contract
        try:
            selectors = get_selectors(filepath + ':' + contract_name)
            if selectors:
                contract_selectors[contract_name] = selectors
        except Exception as oops:
            print(f"Error processing {filepath}: {oops}")


    # Print the final JSON
    solidity_library_code = generate_solidity_function(contract_selectors)
    writeToFile(solidity_library_code)

if __name__ == "__main__":
    main()

