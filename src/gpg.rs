use gpgme::{error::Result, Context, IntoData, PassphraseRequest, PinentryMode, Protocol};
use std::io::Write;

// todo refactor function into signature Read -> Write function
pub fn decrypt<'c, 'p, C, P>(ciphertext_in: C, plaintext_out: P, passphrase: &[u8]) -> Result<()>
where
    C: IntoData<'c>,
    P: IntoData<'p>,
{
    let proto = Protocol::OpenPgp;

    let mut ctx = Context::from_protocol(proto)?;
    ctx.set_pinentry_mode(PinentryMode::Loopback)?;
    ctx.with_passphrase_provider(
        |_: PassphraseRequest, out: &mut dyn Write| {
            out.write_all(passphrase)?;
            Ok(())
        },
        |ctx| ctx.decrypt(ciphertext_in, plaintext_out),
    )?;
    Ok(())
}

// todo refactor function into signature Read -> Write
pub fn encrypt<'p, 'c, P, C>(plaintext_in: P, ciphertext_out: C, passphrase: &[u8]) -> Result<()>
where
    P: IntoData<'p>,
    C: IntoData<'c>,
{
    let proto = Protocol::OpenPgp;

    let mut ctx = Context::from_protocol(proto)?;
    ctx.set_pinentry_mode(PinentryMode::Loopback)?;
    ctx.with_passphrase_provider(
        |_: PassphraseRequest, out: &mut dyn Write| {
            out.write_all(passphrase)?;
            Ok(())
        },
        |ctx| ctx.encrypt_symmetric(plaintext_in, ciphertext_out),
    )?;
    Ok(())
}
