use arcode::bitbit::{BitReader, MSB};
use arcode::{ArithmeticDecoder, ArithmeticEncoder, EOFKind, Model};
use bitbit::BitWriter;
use indexmap::IndexMap;
use std::fs::File;
use std::io::Write;
use std::io::{Cursor, Result};

static MOCK_TEXT: &[u8] = "\
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
"
.as_bytes();

/// Encodes bytes and returns the compressed form
fn encode(data: &[u8]) -> Result<Vec<u8>> {
    let mut k_negative_elements = IndexMap::new();
    let mut k_zero_elements: IndexMap<u8, i32> = IndexMap::new();

    for i in 0..256 {
        k_negative_elements.insert(i as u8, 1);
    }

    let mut probabilities_k_zero: Vec<f32> = vec![];

    for counter in k_zero_elements.values() {
        let counter = *counter as f32;
        let probability = counter / ((k_zero_elements.len() as f32) + 1.0);
        probabilities_k_zero.push(probability);
    }

    let mut model_equiprobable = Model::builder()
        .num_symbols(k_negative_elements.len() as u32)
        .pdf(vec![1.0; k_negative_elements.len()])
        .eof(EOFKind::EndAddOne)
        .build();

    let mut probabilities_k_zero: Vec<f32> = vec![0.0; 256];
    let mut qtd_elements_k_zero = 0;

    for (value, counter) in &k_zero_elements {
        let counter = *counter as f32;
        // +1 no denominador por causa do caracter novo (ou inexistente)
        let probability = counter / ((k_zero_elements.len() as f32) + 1.0);
        probabilities_k_zero[(*value) as usize] = probability;

        if counter >= 1.0 {
            qtd_elements_k_zero = qtd_elements_k_zero + 1;
        }
    }

    let mut model_k_zero: Model = Model::builder()
        .num_symbols(qtd_elements_k_zero)
        .pdf(probabilities_k_zero)
        .eof(EOFKind::EndAddOne)
        .build();

    // make a stream to collect the compressed data
    let compressed = Cursor::new(vec![]);
    let mut compressed_writer = BitWriter::new(compressed);

    let mut encoder = ArithmeticEncoder::new(48);

    for &sym in data {
        model_equiprobable = Model::builder()
            .num_symbols(k_negative_elements.len() as u32)
            .pdf(vec![1.0; k_negative_elements.len()])
            .eof(EOFKind::EndAddOne)
            .build();

        let mut probabilities_k_zero: Vec<f32> = vec![0.0; 256];
        let mut qtd_elements_k_zero = 0;

        for (value, counter) in &k_zero_elements {
            let counter = *counter as f32;
            // +1 no denominador por causa do caracter novo (ou inexistente)
            let probability = counter / ((k_zero_elements.len() as f32) + 1.0);
            probabilities_k_zero[(*value) as usize] = probability;

            if counter >= 1.0 {
                qtd_elements_k_zero = qtd_elements_k_zero + 1;
            }
        }

        model_k_zero = Model::builder()
            .num_symbols(qtd_elements_k_zero)
            .pdf(probabilities_k_zero)
            .eof(EOFKind::EndAddOne)
            .build();

        let symb = sym.into();

        if qtd_elements_k_zero == 0 {
            encoder.encode(sym.into(), &model_equiprobable, &mut compressed_writer)?;

            k_zero_elements.insert(symb, 1);
            k_negative_elements.swap_remove_full(&symb);
            continue;
        }

        if k_zero_elements.contains_key(&symb) {
            println!("{symb}");
            for (el, counter) in &k_zero_elements {
                println!("{el}: {counter}");
            }

            encoder.encode(sym.into(), &model_k_zero, &mut compressed_writer)?;

            if let Some(x) = k_zero_elements.get_mut(&symb) {
                *x = *x + 1;
            }
        } else {
            encoder.encode(sym.into(), &model_equiprobable, &mut compressed_writer)?;

            k_zero_elements.insert(symb, 1);
            k_negative_elements.swap_remove_full(&symb);
        }
        // model.update_symbol(sym.into());
    }

    encoder.encode(model_k_zero.eof(), &model_k_zero, &mut compressed_writer)?;
    encoder.finish_encode(&mut compressed_writer)?;
    compressed_writer.pad_to_byte()?;

    // retrieves the bytes from the writer. This will
    // be cleaner when bitbit updates. Not necessary if
    // using files or a stream
    Ok(compressed_writer.get_ref().get_ref().clone())
}

/// Decompresses the data
fn decode(data: &[u8]) -> Result<Vec<u8>> {
    let mut k_negative_elements = IndexMap::new();
    let mut k_zero_elements: IndexMap<u8, i32> = IndexMap::new();

    for i in 0..256 {
        k_negative_elements.insert(i as u8, 1);
    }

    let mut probabilities_k_zero: Vec<f32> = vec![];

    for counter in k_zero_elements.values() {
        let counter = *counter as f32;
        let probability = counter / k_zero_elements.len() as f32;
        probabilities_k_zero.push(probability);
    }

    let mut model_equiprobable: Model;

    let mut model_k_zero: Model;

    let mut input_reader = BitReader::<_, MSB>::new(data);
    let mut decoder = ArithmeticDecoder::new(48);
    let mut decompressed_data = vec![];

    while !decoder.finished() {
        model_equiprobable = Model::builder()
            .num_symbols(k_negative_elements.len() as u32)
            .pdf(vec![1.0; k_negative_elements.len()])
            .eof(EOFKind::EndAddOne)
            .build();

        let mut probabilities_k_zero: Vec<f32> = vec![0.0; 256];
        let mut qtd_elements_k_zero = 0;

        for (value, counter) in &k_zero_elements {
            let counter = *counter as f32;
            // +1 no denominador por causa do caracter novo (ou inexistente)
            let probability = counter / ((k_zero_elements.len() as f32) + 1.0);
            probabilities_k_zero[(*value) as usize] = probability;

            if counter >= 1.0 {
                qtd_elements_k_zero = qtd_elements_k_zero + 1;
            }
        }

        model_k_zero = Model::builder()
            .num_symbols(qtd_elements_k_zero)
            .pdf(probabilities_k_zero)
            .eof(EOFKind::EndAddOne)
            .build();

        if qtd_elements_k_zero == 0 {
            let sym = decoder.decode(&model_equiprobable, &mut input_reader)?;
            let sym = sym as u8;

            k_zero_elements.insert(sym, 1);
            k_negative_elements.swap_remove_full(&sym);
            continue;
        }

        match decoder.decode(&model_k_zero, &mut input_reader) {
            Ok(sym) => {
                let sym = sym as u8;
                decompressed_data.push(sym);

                // Incrementa um no contador do simbolo
                if let Some(x) = k_zero_elements.get_mut(&sym) {
                    *x = *x + 1;
                }
            }
            Err(_) => match decoder.decode(&model_equiprobable, &mut input_reader) {
                Ok(sym) => {
                    let sym = sym as u8;
                    decompressed_data.push(sym);

                    k_zero_elements.insert(sym, 1);
                    k_negative_elements.swap_remove_full(&sym);
                }
                Err(_) => println!("Error decompressing data"),
            },
        };

        // let sym = decoder.decode(&model_equiprobable, &mut input_reader)?;
        // model.update_symbol(sym);
        // decompressed_data.push(sym as u8);
    }

    decompressed_data.pop(); // remove the EOF

    Ok(decompressed_data)
}

fn main() {
    let compressed = encode(MOCK_TEXT).unwrap();

    let mut file = File::create("Teste.dd").expect("Erro ao criar arquivo Teste.dd");
    match file.write_all(&compressed) {
        Ok(_) => println!("Arquivo comprimido com sucesso!"),
        Err(_) => println!("Erro ao comprimir arquivo"),
    };

    let decompressed = decode(&compressed).unwrap();

    let mut file = File::create("Decompressed.txt").expect("Erro ao criar arquivo Decompressed.dd");
    match file.write_all(&decompressed) {
        Ok(_) => println!("Arquivo descomprimido com sucesso!"),
        Err(_) => println!("Erro ao descomprimir arquivo"),
    };
}
