use async_std::prelude::*;

async fn r<F, O>(
    file_path: &std::path::Path,
    delay: std::time::Duration,
    cb: &dyn Fn(async_std::fs::File) -> F,
) -> std::io::Result<O>
where
    F: Future<Output = std::io::Result<O>>,
{
    let orig_mtime = std::fs::metadata(file_path)?.modified()?;

    async_std::task::sleep(delay).await;

    let f = async_std::fs::File::open(file_path).await?;

    let i = cb(f).await?;

    let after_mtime = std::fs::metadata(file_path)?.modified()?;
    if after_mtime != orig_mtime {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "oh no"));
    }

    Ok(i)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_fileread() -> std::io::Result<()> {
        {
            let mut file = std::fs::File::create("foo.txt")?;
        }
        let writer = async_std::task::spawn(async {
            for c in 'a'..'z' {
                {
                    let mut file = async_std::fs::File::create("foo.txt").await.unwrap();
                    let st = String::from(c);
                    let s = str::repeat(&st, 100_000);
                    file.write_all(s.as_bytes()).await.unwrap();
                }
                println!("{}", c);
                async_std::task::sleep(std::time::Duration::from_millis(
                    rand::random::<u64>() % 100,
                ))
                .await;
            }
        });

        let mut c: char = '0';
        while c != 'y' {
            let res = async_std::task::block_on(async {
                r(
                    std::path::Path::new("foo.txt"),
                    std::time::Duration::from_millis(20),
                    &|mut rd: async_std::fs::File| async move {
                        let mut s = String::new();
                        rd.read_to_string(&mut s).await.unwrap();
                        //println!("read: {}", s);
                        let fs = s
                            .chars()
                            .next()
                            .ok_or(std::io::Error::new(std::io::ErrorKind::Other, "no first"))?;
                        assert!(s.chars().all(|ch| ch == fs));

                        Ok(fs)
                    },
                )
                .await
            });

            match res {
                Err(ref e) if e.kind() == std::io::ErrorKind::Other => {
                    println!("changed")
                }
                Ok(i) => {
                    c = i;
                    println!("set to {}", i)
                }
                Err(e) => panic!(e),
            }
        }

        async_std::task::block_on(writer);
        std::fs::remove_file("foo.txt")?;

        Ok(())
    }
}
