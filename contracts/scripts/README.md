# Before deploying:

-   change .env.template to .env
-   fill in your own values for private key and rpc url (for calibrationnet)

# To deploy everything run:

```bash
npx hardhat deploy
```

## To deploy only the libraries:

```bash
npx hardhat deploy-libraries
```

## To deploy only the Gateway:

```bash
npx hardhat deploy-gateway
```

## To deploy only the Gateway Actor:

```bash
npx hardhat deploy-gateway
```

## To deploy only the Registry:

```bash
npx hardhat run scripts/deploy-registry.ts
```
