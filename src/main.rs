use std::{
    fs::File,
    io::{Cursor, Result, Write},
};

use arcode::{ArithmeticDecoder, ArithmeticEncoder, EOFKind, Model};
use bitbit::{BitReader, BitWriter, MSB};
use indexmap::IndexMap;

const MAX_K_CONTEXT: usize = 2;

static MOCK_TEXT: &str = "\
[Verse 1: Aviya Dor-Kolan]
I don't know what I was thinking, leaving my child behind
Now I suffer the curse, and now I am blind
With all this anger, guilt, and sadness coming to haunt me forever
I can't wait for the cliff at the end of the river

[Verse 2: Aviya Dor-Kolan]
Is this revenge I am seeking, or seeking someone to avenge me?
Stuck in my own paradox, I wanna set myself free
Maybe I should chase and find before they'll try to stop it
It won't be long before I'll become a puppet

[Chorus: Aviya Dor-Kolan]
It's been so long
Since I last have seen my son
Lost to this monster
To the man behind the slaughter
Since you've been gone
I've been singing this stupid song
So I could ponder
The sanity of your mother

[Instrumental Interlude]

[Verse 3: Aviya Dor-Kolan]
I wish I lived in the present with the gift of my past mistakes
But the future keeps luring in like a pack of snakes
Your sweet little eyes, your little smile is all I remember
Those fuzzy memories mess with my temper

[Verse 4: Aviya Dor-Kolan]
Justification is killing me, but killing isn't justified
What happened to my son? I'm terrified
It lingers in my mind, and the thought keeps on getting bigger
I'm sorry, my sweet baby, I wish I'd been there

[Chorus: Aviya Dor-Kolan]
It's been so long
Since I last have seen my son
Lost to this monster
To the man behind the slaughter
Since you've been gone
I've been singing this stupid song
So I could ponder
The sanity of your mother

[Instrumental Outro]
";

/// Encodes bytes and returns the compressed form
fn encode(data: &[u8]) -> Result<Vec<u8>> {
    let mut k_symbols_models: Vec<IndexMap<u8, i32>> = vec![];

    for _ in 0..MAX_K_CONTEXT {
        k_symbols_models.push(IndexMap::new());
    }

    for i in 0..256 {
        let i = i as u8;
        k_symbols_models[0].insert(i, 1);
    }

    // make a stream to collect the compressed data
    let compressed = Cursor::new(vec![]);
    let mut compressed_writer = BitWriter::new(compressed);

    let mut encoder = ArithmeticEncoder::new(48);

    'encoder_loop: for &sym in data {
        let mut models: Vec<Model> = vec![];

        for i in 0..MAX_K_CONTEXT {
            let model = Model::builder()
                .num_symbols(k_symbols_models[i].len() as u32)
                .eof(EOFKind::EndAddOne)
                .build();

            models.push(model);
        }

        for _ in 1..MAX_K_CONTEXT {
            let mut i = 0;
            for (_, counter) in &k_symbols_models[1] {
                for _ in 1..(*counter) {
                    models[1].update_symbol(i);
                }

                i = i + 1;
            }
        }

        for i in (1..MAX_K_CONTEXT).rev() {
            if k_symbols_models[i - 1].len() == 0 {
                continue;
            }

            if k_symbols_models[i].len() == 0 {
                for j in (0..i).rev() {
                    if !k_symbols_models[j].contains_key(&sym) {
                        k_symbols_models[j][&255] = k_symbols_models[j][&255] + 1;
                        continue;
                    }

                    let el_index = k_symbols_models[j].get_index_of(&sym).unwrap();
                    let el_index = el_index as u8;

                    encoder.encode(el_index.into(), &models[j], &mut compressed_writer)?;

                    if j == 0 {
                        k_symbols_models[j].swap_remove(&sym);
                    } else {
                        k_symbols_models[j][&sym] = k_symbols_models[j][&sym] + 1;
                    }
                }

                let rho: u8 = 255;
                let eof: u8 = 254;
                k_symbols_models[i].insert(sym, 1);
                k_symbols_models[i].insert(rho, 1);
                k_symbols_models[i].insert(eof, 1);

                continue 'encoder_loop;
            }
        }

        for i in (0..MAX_K_CONTEXT).rev() {
            if k_symbols_models[i].contains_key(&sym) {
                let el_index = k_symbols_models[i].get_index_of(&sym).unwrap();
                let el_index = el_index as u8;

                encoder.encode(el_index.into(), &models[i], &mut compressed_writer)?;
                if i == 0 {
                    k_symbols_models[i].swap_remove_entry(&sym);
                } else {
                    k_symbols_models[i][&sym] = k_symbols_models[i][&sym] + 1;
                }

                continue 'encoder_loop;
            } else {
                let rho: u8 = 255;
                let el_index = k_symbols_models[i].get_index_of(&rho).unwrap();
                let el_index = el_index as u8;

                encoder.encode(el_index.into(), &models[1], &mut compressed_writer)?;

                k_symbols_models[1][&rho] = k_symbols_models[1][&rho] + 1;
                k_symbols_models[1].insert(sym, 1);
            }
        }
    }

    let mut model_k0 = Model::builder()
        .num_symbols(k_symbols_models[1].len() as u32)
        .eof(EOFKind::EndAddOne)
        .build();

    let mut i = 0;
    for (_, counter) in &k_symbols_models[1] {
        for _ in 1..(*counter) {
            model_k0.update_symbol(i);
        }

        i = i + 1;
    }

    let eof = 254;
    let el_index = k_symbols_models[1].get_index_of(&eof).unwrap();
    let el_index = el_index as u8;

    encoder.encode(el_index.into(), &model_k0, &mut compressed_writer)?;
    encoder.finish_encode(&mut compressed_writer)?;
    compressed_writer.pad_to_byte()?;

    // retrieves the bytes from the writer. This will
    // be cleaner when bitbit updates. Not necessary if
    // using files or a stream
    Ok(compressed_writer.get_ref().get_ref().clone())
}

/// Decompresses the data
fn decode(data: &[u8]) -> Result<Vec<u8>> {
    let mut k_symbols_models: Vec<IndexMap<u8, i32>> = vec![];

    for _ in 0..MAX_K_CONTEXT {
        k_symbols_models.push(IndexMap::new());
    }

    for i in 0..256 {
        let i = i as u8;
        k_symbols_models[0].insert(i, 1);
    }

    let mut input_reader = BitReader::<_, MSB>::new(data);
    let mut decoder = ArithmeticDecoder::new(48);
    let mut decompressed_data = vec![];

    'decoder_loop: while !decoder.finished() {
        let mut models: Vec<Model> = vec![];

        for i in 0..MAX_K_CONTEXT {
            let model = Model::builder()
                .num_symbols(k_symbols_models[i].len() as u32)
                .eof(EOFKind::EndAddOne)
                .build();

            models.push(model);
        }

        for k in 1..MAX_K_CONTEXT {
            let mut i = 0;
            for (_, counter) in &k_symbols_models[k] {
                for _ in 1..(*counter) {
                    models[k].update_symbol(i);
                }

                i = i + 1;
            }
        }

        for i in (1..MAX_K_CONTEXT).rev() {
            if k_symbols_models[i - 1].len() == 0 {
                continue;
            }

            if k_symbols_models[i].len() == 0 {
                let idx = decoder.decode(&models[i - 1], &mut input_reader)?;

                let value = k_symbols_models[i - 1].get_index(idx as usize).unwrap();
                let sym = *value.0;

                if sym == 255 {
                    k_symbols_models[i][&255] = k_symbols_models[i][&255] + 1;
                }

                let mut j = i - 1;
                while sym == 255 {
                    let idx = decoder.decode(&models[j], &mut input_reader)?;

                    let value = k_symbols_models[j].get_index(idx as usize).unwrap();
                    let sym = *value.0;

                    if sym == 255 {
                        k_symbols_models[j][&255] = k_symbols_models[j][&255] + 1;
                    }

                    j = j + 1;
                }

                decompressed_data.push(sym);
                if j == 0 {
                    k_symbols_models[i - 1].swap_remove_entry(&sym);
                } else {
                    k_symbols_models[j][&sym] = k_symbols_models[j][&sym] + 1;
                }

                for k in (j + 1)..i {
                    k_symbols_models[k].insert(sym, 1);
                }

                let rho: u8 = 255;
                let eof: u8 = 254;
                k_symbols_models[i].insert(sym, 1);
                k_symbols_models[i].insert(rho, 1);
                k_symbols_models[i].insert(eof, 1);

                continue 'decoder_loop;
            }

            break;
        }

        let mut model_k = MAX_K_CONTEXT - 1;
        let idx = decoder.decode(&models[model_k], &mut input_reader)?;
        let value = k_symbols_models[model_k].get_index(idx as usize).unwrap();
        let sym = *value.0;

        // Verifica se o simbolo codificado é EOF
        if sym == 254 {
            decoder.set_finished();
            continue;
        }

        if sym == 255 {
            model_k = model_k - 1;
            let initial_model_k = model_k;
            let idx = decoder.decode(&models[model_k], &mut input_reader)?;
            let value = k_symbols_models[model_k].get_index(idx as usize).unwrap();
            let mut sym = *value.0;

            if sym != 255 {
                decompressed_data.push(sym);

                if model_k == 0 {
                    k_symbols_models[0].swap_remove_entry(&sym);
                } else {
                    k_symbols_models[model_k][&sym] = k_symbols_models[model_k][&sym] + 1;
                }

                k_symbols_models[model_k + 1][&(255 as u8)] =
                    k_symbols_models[model_k + 1][&(255 as u8)] + 1;
                k_symbols_models[model_k + 1].insert(sym, 1);
                continue 'decoder_loop;
            }

            while sym == 255 {
                k_symbols_models[model_k][&(255 as u8)] =
                    k_symbols_models[model_k][&(255 as u8)] + 1;

                model_k = model_k - 1;

                let idx = decoder.decode(&models[model_k], &mut input_reader)?;
                let value = k_symbols_models[model_k].get_index(idx as usize).unwrap();
                sym = *value.0;
            }

            decompressed_data.push(sym);

            if model_k == 0 {
                k_symbols_models[0].swap_remove_entry(&sym);
            } else {
                k_symbols_models[model_k][&sym] = k_symbols_models[model_k][&sym] + 1;
            }

            // k_symbols_models[model_k + 1][&(255 as u8)] =
            //     k_symbols_models[model_k + 1][&(255 as u8)] + 1;
            for m in (model_k + 1)..initial_model_k {
                k_symbols_models[m].insert(sym, 1);
            }

            continue 'decoder_loop;
        }

        decompressed_data.push(sym);
        k_symbols_models[1][&sym] = k_symbols_models[1][&sym] + 1;
    }

    decompressed_data.pop(); // remove the EOF

    Ok(decompressed_data)
}

fn main() {
    let sample_bytes = MOCK_TEXT.bytes().into_iter().collect::<Vec<u8>>();
    let compressed = encode(&sample_bytes).unwrap();
    let decompressed = decode(&compressed).unwrap();

    let mut f = File::create("Compressed.dd").expect("Erro ao criar arquivo");
    match f.write_all(&compressed) {
        Ok(_) => println!("Comprimido com sucesso!"),
        Err(_) => println!("Erro ao salvar arquivo comprimido"),
    };

    let mut f = File::create("Decompressed.txt").expect("Erro ao criar arquivo");
    match f.write_all(&decompressed) {
        Ok(_) => println!("Descomprimido com sucesso!"),
        Err(_) => println!("Erro ao comprimir arquivo"),
    }
}
