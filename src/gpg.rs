use gpgme::{error::Result, Context, PassphraseRequest, PinentryMode, Protocol};
use std::{fs::File, io::Write};

// todo make Read -> Write function
pub fn decrypt(data: &mut File, passphrase: &[u8]) -> Result<Vec<u8>> {
    let proto = Protocol::OpenPgp;

    let mut output = Vec::new();
    let mut ctx = Context::from_protocol(proto)?;
    ctx.set_pinentry_mode(PinentryMode::Loopback)?;
    ctx.with_passphrase_provider(
        |_: PassphraseRequest, out: &mut dyn Write| {
            out.write_all(passphrase)?;
            Ok(())
        },
        |ctx| ctx.decrypt(data, &mut output),
    )?;
    Ok(output)
}

// todo make Read -> Write function
pub fn encrypt(data: &mut File, passphrase: &[u8]) -> Result<Vec<u8>> {
    Ok(vec![])
}
