# Properties

List of identified and checked invariants of the IPC protocol following the categorization by [Certora](https://github.com/Certora/Tutorials/blob/master/06.Lesson_ThinkingProperties/Categorizing_Properties.pdf):

-   Valid States
-   State Transitions
-   Variable Transitions
-   High-Level Properties
-   Unit Tests
-   Valid States
-   State Transitions
-   Variable Transitions
-   High-Level Properties
-   Unit Tests

## Subnet Registry

| Property | Description                                               | Category             | Tested |
| -------- | --------------------------------------------------------- | -------------------- | ------ |
| SR-01    | The Gateway address is not changed                        | Variable Transitions | ✅     |
| SR-02    | If a subnet was created then its address can be retrieved | High Level           | ✅     |

## Subnet Actor

| Property | Description                                                                                           | Category             | Tested |
| -------- | ----------------------------------------------------------------------------------------------------- | -------------------- | ------ |
| SA-01    | The number of joined validators is equal to the number of total validators.                           | Variable Transitions | ✅     |
| SA-02    | The stake of the subnet is the same from the GatewayActor and SubnetActor perspective.                | Unit Test            | ✅     |
| SA-03    | The value resulting from all stake and unstake operations is equal to the total confirmed collateral. | Valid State          | ✅     |
| SA-04    | After leaving the subnet, a validator can claim their collateral.                                     | High Level           | ✅     |
| SA-05    | Total confirmed collateral equals sum of validator collaterals.                                       | Valid State          | ✅     |
