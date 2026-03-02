use std::{
    fs::File,
    io::{Cursor, Result, Write},
};

use arcode::{ArithmeticDecoder, ArithmeticEncoder, EOFKind, Model};
use bitbit::{BitReader, BitWriter, MSB};
use indexmap::IndexMap;

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
    let mut k_negative_symbols: IndexMap<u8, i32> = IndexMap::new();
    let mut k_0_symbols: IndexMap<u8, i32> = IndexMap::new();

    for i in 0..256 {
        let i = i as u8;
        k_negative_symbols.insert(i, 1);
    }

    // make a stream to collect the compressed data
    let compressed = Cursor::new(vec![]);
    let mut compressed_writer = BitWriter::new(compressed);

    let mut encoder = ArithmeticEncoder::new(48);

    for &sym in data {
        let model = Model::builder()
            .num_symbols(k_negative_symbols.len() as u32)
            .eof(EOFKind::EndAddOne)
            .build();

        let mut model_k0 = Model::builder()
            .num_symbols(k_0_symbols.len() as u32)
            .eof(EOFKind::EndAddOne)
            .build();

        let mut i = 0;
        for (_, counter) in &k_0_symbols {
            for _ in 1..(*counter) {
                model_k0.update_symbol(i);
            }

            i = i + 1;
        }

        if k_0_symbols.len() == 0 {
            let el_index = k_negative_symbols.get_index_of(&sym).unwrap();
            let el_index = el_index as u8;

            encoder.encode(el_index.into(), &model, &mut compressed_writer)?;

            let rho: u8 = 255;
            let eof: u8 = 254;
            k_0_symbols.insert(sym, 1);
            k_0_symbols.insert(rho, 1);
            k_0_symbols.insert(eof, 1);

            k_negative_symbols.swap_remove(&sym);

            continue;
        }

        if k_0_symbols.contains_key(&sym) {
            let el_index = k_0_symbols.get_index_of(&sym).unwrap();
            let el_index = el_index as u8;

            encoder.encode(el_index.into(), &model_k0, &mut compressed_writer)?;
            k_0_symbols[&sym] = k_0_symbols[&sym] + 1;
        } else if k_negative_symbols.contains_key(&sym) {
            let rho: u8 = 255;
            let el_index = k_0_symbols.get_index_of(&rho).unwrap();
            let el_index = el_index as u8;

            encoder.encode(el_index.into(), &model_k0, &mut compressed_writer)?;

            let el_index = k_negative_symbols.get_index_of(&sym).unwrap();
            let el_index = el_index as u8;

            encoder.encode(el_index.into(), &model, &mut compressed_writer)?;

            k_negative_symbols.swap_remove_entry(&sym);
            k_0_symbols[&rho] = k_0_symbols[&rho] + 1;
            k_0_symbols.insert(sym, 1);
        }
    }

    let mut model_k0 = Model::builder()
        .num_symbols(k_0_symbols.len() as u32)
        .eof(EOFKind::EndAddOne)
        .build();

    let mut i = 0;
    for (_, counter) in &k_0_symbols {
        for _ in 1..(*counter) {
            model_k0.update_symbol(i);
        }

        i = i + 1;
    }

    let eof = 254;
    let el_index = k_0_symbols.get_index_of(&eof).unwrap();
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
    let mut k_negative_symbols: IndexMap<u8, i32> = IndexMap::new();
    let mut k_0_symbols: IndexMap<u8, i32> = IndexMap::new();

    for i in 0..256 {
        let i = i as u8;
        k_negative_symbols.insert(i, 1);
    }

    let mut input_reader = BitReader::<_, MSB>::new(data);
    let mut decoder = ArithmeticDecoder::new(48);
    let mut decompressed_data = vec![];

    while !decoder.finished() {
        let model = Model::builder()
            .num_symbols(k_negative_symbols.len() as u32)
            .eof(EOFKind::EndAddOne)
            .build();

        let mut model_k0 = Model::builder()
            .num_symbols(k_0_symbols.len() as u32)
            .eof(EOFKind::EndAddOne)
            .build();

        let mut i = 0;
        for (_, counter) in &k_0_symbols {
            for _ in 1..(*counter) {
                model_k0.update_symbol(i);
            }
            i = i + 1;
        }

        if k_0_symbols.len() == 0 {
            let idx = decoder.decode(&model, &mut input_reader)?;

            let value = k_negative_symbols.get_index(idx as usize).unwrap();
            let sym = *value.0;

            decompressed_data.push(sym);
            k_negative_symbols.swap_remove_entry(&sym);

            let rho: u8 = 255;
            let eof: u8 = 254;
            k_0_symbols.insert(sym, 1);
            k_0_symbols.insert(rho, 1);
            k_0_symbols.insert(eof, 1);

            continue;
        }

        let idx = decoder.decode(&model_k0, &mut input_reader)?;
        let value = k_0_symbols.get_index(idx as usize).unwrap();
        let sym = *value.0;

        // Verifica se o simbolo codificado é EOF
        if sym == 254 {
            decoder.set_finished();
            continue;
        }

        // Se o simbolo for rho
        if sym == 255 {
            let idx = decoder.decode(&model, &mut input_reader)?;
            let value = k_negative_symbols.get_index(idx as usize).unwrap();
            let sym = *value.0;
            // let test = sym as char;
            // println!("{test}");

            decompressed_data.push(sym);

            k_negative_symbols.swap_remove_entry(&sym);

            k_0_symbols[&(255 as u8)] = k_0_symbols[&(255 as u8)] + 1;
            k_0_symbols.insert(sym, 1);
            continue;
        }

        decompressed_data.push(sym);
        k_0_symbols[&sym] = k_0_symbols[&sym] + 1;
    }

    decompressed_data.pop(); // remove the EOF

    Ok(decompressed_data)
}

fn main() {
    for i in 1..1 {
        println!("{i}")
    }
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
        Ok(_) => println!("Comprimido com sucesso"),
        Err(_) => println!("Erro ao comprimir arquivo"),
    }
}
