use arcode::bitbit::{BitReader, MSB};
use arcode::{ArithmeticDecoder, ArithmeticEncoder, EOFKind, Model};
use bitbit::BitWriter;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::io::{Cursor, Result};

#[derive(PartialEq, Eq)]
enum KValueType {
    KNegative,
    KNonNegative,
}

/// Encodes bytes and returns the compressed form
fn encode(data: &[u8], qtd_symbols: u32, encode_eof: bool, k_value: KValueType) -> Result<Vec<u8>> {
    let mut model = match k_value {
        KValueType::KNegative => Model::builder()
            .num_symbols(qtd_symbols)
            .eof(EOFKind::EndAddOne)
            .pdf(Vec::from(vec![1.0; qtd_symbols as usize]))
            .build(),
        KValueType::KNonNegative => Model::builder()
            .num_symbols(qtd_symbols)
            .eof(EOFKind::EndAddOne)
            .pdf(Vec::from(vec![1.0; qtd_symbols as usize]))
            .build(),
    };

    // make a stream to collect the compressed data
    let compressed = Cursor::new(vec![]);
    let mut compressed_writer = BitWriter::new(compressed);

    let mut encoder = ArithmeticEncoder::new(48);
    for &sym in data {
        encoder.encode(sym.into(), &model, &mut compressed_writer)?;
        if k_value == KValueType::KNonNegative {
            model.update_symbol(sym.into());
        }
    }

    if encode_eof {
        encoder.encode(model.eof(), &model, &mut compressed_writer)?;
    }
    encoder.finish_encode(&mut compressed_writer)?;
    compressed_writer.pad_to_byte()?;

    // retrieves the bytes from the writer. This will
    // be cleaner when bitbit updates. Not necessary if
    // using files or a stream
    Ok(compressed_writer.get_ref().get_ref().clone())
}

/// Decompresses the data
fn decode(data: &[u8], qtd_symbols: u32, k_value: KValueType) -> Result<Vec<u8>> {
    let mut model = match k_value {
        KValueType::KNegative => Model::builder()
            .num_symbols(qtd_symbols)
            .eof(EOFKind::EndAddOne)
            .pdf(Vec::from(vec![1.0; qtd_symbols as usize]))
            .build(),
        KValueType::KNonNegative => Model::builder()
            .num_symbols(qtd_symbols)
            .eof(EOFKind::EndAddOne)
            .pdf(Vec::from(vec![1.0; qtd_symbols as usize]))
            .build(),
    };

    let mut input_reader = BitReader::<_, MSB>::new(data);
    let mut decoder = ArithmeticDecoder::new(48);
    let mut decompressed_data = vec![];

    while !decoder.finished() {
        let sym = decoder.decode(&model, &mut input_reader)?;
        if k_value == KValueType::KNonNegative {
            model.update_symbol(sym.into());
        }
        decompressed_data.push(sym as u8);
    }

    decompressed_data.pop(); // remove the EOF

    Ok(decompressed_data)
}

fn main() {
    //let mut neg_k_symbols = HashMap::new();

    let sample_text = "\
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

    let mut model = Model::builder().num_bits(8).eof(EOFKind::EndAddOne).build();

    // make a stream to collect the compressed data
    let compressed = Cursor::new(vec![]);
    let mut compressed_writer = BitWriter::new(compressed);

    let mut encoder = ArithmeticEncoder::new(48);

    for &sym in sample_text {
        encoder
            .encode(sym.into(), &model, &mut compressed_writer)
            .expect("Erro ao tentar codificar simbolo {sym}");
        model.update_symbol(sym.into());
    }

    encoder
        .encode(model.eof(), &model, &mut compressed_writer)
        .expect("Erro ao codificar EOF");
    encoder
        .finish_encode(&mut compressed_writer)
        .expect("Erro ao finalizar codificação");
    compressed_writer
        .pad_to_byte()
        .expect("Erro ao realizar padding");

    let compressed = compressed_writer.get_ref().get_ref().clone();

    let mut file = File::create("Teste.dd").expect("Não foi possível abrir o arquivo :(");
    match file.write_all(&compressed) {
        Ok(_) => println!("Arquivo escrito com sucesso!"),
        Err(_) => println!("Erro ao escrever o arquivo"),
    };

    let mut input_reader = BitReader::<_, MSB>::new(compressed);
    let mut decoder = ArithmeticDecoder::new(48);
    let mut decompressed_data = vec![];

    while !decoder.finished() {
        let sym = decoder
            .decode(&model, &mut input_reader)
            .except("Erro tentando decodificar simbolo");
        model.update_symbol(sym);
        decompressed_data.push(sym as u8);
    }

    decompressed_data.pop(); // remove the EOF

    // for sym in compressed {
    //     let v = vec![sym as u8];

    //     match decode(&v, 256, KValueType::KNegative) {
    //         Ok(mut symb) => decompressed.append(&mut symb),
    //         Err(_) => println!("Erro ao tentar decodificar"),
    //     }
    // }

    let mut file = File::create("Decompressed.txt").expect("Não foi possível abrir o arquivo :(");
    match file.write_all(&decompressed_data) {
        Ok(_) => println!("Arquivo escrito com sucesso!"),
        Err(_) => println!("Erro ao escrever o arquivo"),
    };

    println!("Hello, world!");
}
