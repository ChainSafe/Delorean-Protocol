// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fs::File;
use std::io::Result;

/// Restricts permissions on a file to user-only: 0600
#[cfg(unix)]
pub fn set_user_perm(file: &File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    use log::info;

    let mut perm = file.metadata()?.permissions();
    #[allow(clippy::useless_conversion)] // Otherwise it does not build on macos
    perm.set_mode((libc::S_IWUSR | libc::S_IRUSR).into());
    file.set_permissions(perm)?;

    info!("Permissions set to 0600 on {:?}", file);

    Ok(())
}
