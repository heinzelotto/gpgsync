use std::io::Write;

// TODO refactor function into signature Read -> Write function
pub fn decrypt<'c, 'p, C, P>(
    ciphertext_in: C,
    plaintext_out: P,
    passphrase: &[u8],
) -> gpgme::error::Result<()>
where
    C: gpgme::IntoData<'c>,
    P: gpgme::IntoData<'p>,
{
    let proto = gpgme::Protocol::OpenPgp;

    let mut ctx = gpgme::Context::from_protocol(proto)?;
    ctx.set_flag("no-symkey-cache\0", "1\0").unwrap();
    ctx.set_pinentry_mode(gpgme::PinentryMode::Loopback)?;
    ctx.with_passphrase_provider(
        |_: gpgme::PassphraseRequest, out: &mut dyn Write| {
            out.write_all(passphrase)?;
            Ok(())
        },
        |ctx| ctx.decrypt(ciphertext_in, plaintext_out),
    )?;
    Ok(())
}

// TODO refactor function into signature Read -> Write
pub fn encrypt<'p, 'c, P, C>(
    plaintext_in: P,
    ciphertext_out: C,
    passphrase: &[u8],
) -> gpgme::error::Result<()>
where
    P: gpgme::IntoData<'p>,
    C: gpgme::IntoData<'c>,
{
    let proto = gpgme::Protocol::OpenPgp;

    let mut ctx = gpgme::Context::from_protocol(proto)?;
    ctx.set_flag("no-symkey-cache\0", "1\0").unwrap();
    ctx.set_pinentry_mode(gpgme::PinentryMode::Loopback)?;
    ctx.with_passphrase_provider(
        |_: gpgme::PassphraseRequest, out: &mut dyn Write| {
            out.write_all(passphrase)?;
            Ok(())
        },
        |ctx| ctx.encrypt_symmetric(plaintext_in, ciphertext_out),
    )?;
    Ok(())
}

// TODO unit tests
