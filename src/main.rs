use arcode::{ArithmeticDecoder, ArithmeticEncoder, EOFKind, Model};
use bitbit::{BitReader, BitWriter, MSB};
use std::{
    cmp::min,
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Cursor, Result, Write},
};

const RHO: u32 = 256;
const EOF_SYMBOL: u32 = 257;

struct Context {
    model: Model,
    seen_symbols: HashSet<u32>,
}

impl Context {
    fn new() -> Self {
        let mut model = Model::builder()
            .num_symbols(258)
            .eof(EOFKind::EndAddOne)
            .build();

        model.update_symbol(RHO);

        Self {
            model,
            seen_symbols: HashSet::new(),
        }
    }
}

struct PPMC {
    contexts: HashMap<Vec<u8>, Context>,
    minus_one: Model,
    max_n: usize,
}

impl PPMC {
    fn new(max_n: usize) -> Self {
        let minus_one = Model::builder()
            .num_symbols(258)
            .eof(EOFKind::EndAddOne)
            .build();

        Self {
            contexts: HashMap::new(),
            minus_one,
            max_n,
        }
    }

    fn encode(
        &mut self,
        data: &[u8],
        encoder: &mut ArithmeticEncoder,
        writer: &mut BitWriter<Cursor<Vec<u8>>>,
    ) -> Result<()> {
        let mut metrics_file = File::create(format!("metrics_order_{}.csv", self.max_n))
            .expect("Erro ao escrever arquivo de métrica");
        metrics_file.write(b"n,comprimento_medio\n")?;
        let mut metrics: Vec<(i32, f64)> = vec![];
        // Armazena os N últimos caracteres
        // Ex: N = 3 ['e', 'n', 's']
        let mut last_characters: Vec<u8> = Vec::with_capacity(self.max_n);

        let mut last_mean_progressive_length = 0.0;

        let mut current_pos = 1;
        for &byte in data {
            let symbol = byte as u32;
            let mut encoded = false;
            // min() é usado para codificar corretamente os N primeiros bytes
            let mut current_n = min(last_characters.len(), self.max_n);

            loop {
                // Extrai a sequência de caracteres a ser observada no contexto atual
                let context_key = last_characters[last_characters.len() - current_n..].to_vec();

                // Procura a sequência de caracteres no contexto atual, se não
                // a encontrar, adiciona ao contexto.
                // Presente: node contem uma referência que aponta para o contexto
                //           presente dentro do Hash Map
                // Não Presente: node contêm uma referência que aponta para o novo
                //               contexto criado
                let node = self
                    .contexts
                    .entry(context_key.clone())
                    .or_insert_with(Context::new);

                // Verifica se o símbolo sendo processado já foi visto depois
                // do contexto atual
                if node.seen_symbols.contains(&symbol) {
                    // Se sim, codifica e vai para o próximo símbolo
                    encoder.encode(symbol, &node.model, writer)?;
                    encoded = true;
                    break;
                } else {
                    // Se não, codifica o escape e desce de contexto
                    encoder.encode(RHO, &node.model, writer)?;
                }
                if current_n == 0 {
                    break;
                }
                current_n -= 1;
            }

            // Se o símbolo nunca foi encontrado, codifica para N = -1
            if !encoded {
                encoder.encode(symbol, &self.minus_one, writer)?;
            }

            // Atualização dos contextos
            for order in 0..=min(last_characters.len(), self.max_n) {
                // Define contexto atual
                let context_key = last_characters[last_characters.len() - order..].to_vec();
                // Atualiza contexto atual
                if let Some(node) = self.contexts.get_mut(&context_key) {
                    node.model.update_symbol(symbol);
                    node.seen_symbols.insert(symbol);
                }
            }

            // Add metrics to file
            let total_attributed_bits = writer.get_ref().position();
            let mean_progressive_length = (total_attributed_bits as f64) / current_pos as f64;
            metrics.push((current_pos, mean_progressive_length));

            if current_pos % 1000 == 0 {
                if last_mean_progressive_length != 0.0 {
                    let percentile_mpl_upper_bound = 0.01 * last_mean_progressive_length;

                    // If the current mean progressive length is bigger than 1 percent of the last one
                    if (mean_progressive_length - last_mean_progressive_length)
                        > percentile_mpl_upper_bound
                    {
                        self.contexts.clear();
                        last_characters = Vec::with_capacity(self.max_n);
                    }
                }

                last_mean_progressive_length = mean_progressive_length;
            }

            // Avança para o próximo símbolo
            last_characters.push(byte);
            if last_characters.len() > self.max_n {
                last_characters.remove(0);
            }

            current_pos = current_pos + 1;
        }

        for metric in metrics {
            metrics_file.write((format!("{},{}\n", metric.0, metric.1)).as_bytes())?;
        }

        Ok(())
    }

    fn decode(
        &mut self,
        decoder: &mut ArithmeticDecoder,
        reader: &mut BitReader<Cursor<Vec<u8>>, MSB>,
    ) -> Result<Vec<u8>> {
        let mut output: Vec<u8> = Vec::new();
        // Armazena os N últimos caracteres
        let mut last_characters: Vec<u8> = Vec::with_capacity(self.max_n);

        let mut last_mean_progressive_length = 0.0;

        let mut current_pos = 1;

        'decode_loop: loop {
            // min() é usado para decodificar corretamente os N primeiros bytes
            let mut current_n = std::cmp::min(last_characters.len(), self.max_n);
            let mut decoded_symbol: u32 = 0;
            let mut found = false;

            loop {
                // Extrai a sequência de caracteres a ser observada no contexto atual
                let context_key = last_characters[last_characters.len() - current_n..].to_vec();

                // Procura a sequência de caracteres a ser observada no contexto atual,
                // se não a encontrar, adiciona ao contexto
                let node = self
                    .contexts
                    .entry(context_key.clone())
                    .or_insert_with(Context::new);

                // Lê o stream e decodifica um símbolo baseado no modelo deste contexto
                let symbol = decoder.decode(&node.model, reader)?;

                // Verifica se o símbolo decodificado é o escape
                if symbol == RHO {
                    if current_n == 0 {
                        // Se for escape no contexto 0, sai do loop para buscar
                        // no contexto -1
                        break;
                    }
                    // Desce de contexto
                    current_n -= 1;
                } else {
                    // Se não for escape, encontramos o símbolo decodificado
                    decoded_symbol = symbol;
                    found = true;
                    break;
                }
            }

            // Se não foi encontrado antes, decodifica para N = -1
            if !found {
                decoded_symbol = decoder.decode(&self.minus_one, reader)?;
            }

            // Verifica se a compressão acabou
            if decoded_symbol == EOF_SYMBOL {
                decoder.set_finished();
                break 'decode_loop;
            }

            // Adiciona o byte decodificado a saída final
            let byte = decoded_symbol as u8;
            output.push(byte);

            for order in 0..=min(last_characters.len(), self.max_n) {
                // Define contexto atual
                let context_key = last_characters[last_characters.len() - order..].to_vec();
                // Atualiza contexto atual
                if let Some(node) = self.contexts.get_mut(&context_key) {
                    node.model.update_symbol(decoded_symbol);
                    node.seen_symbols.insert(decoded_symbol);
                }
            }

            // Each time it decodes
            let total_attributed_bits = reader.get_ref().position();
            let mean_progressive_length = (total_attributed_bits as f64) / current_pos as f64;

            if current_pos % 1000 == 0 {
                if last_mean_progressive_length != 0.0 {
                    let percentile_mpl_upper_bound = 0.01 * last_mean_progressive_length;

                    // If the current mean progressive length is bigger than 1 percent of the last one
                    if (mean_progressive_length - last_mean_progressive_length)
                        > percentile_mpl_upper_bound
                    {
                        self.contexts.clear();
                        last_characters = Vec::with_capacity(self.max_n);
                    }
                }

                last_mean_progressive_length = mean_progressive_length;
            }

            // Avança para o próximo símbolo
            last_characters.push(byte);
            if last_characters.len() > self.max_n {
                last_characters.remove(0);
            }
            current_pos = current_pos + 1;
        }

        Ok(output)
    }
}

// fn main() {
//     let input_path = "corpus/dickens";

//     println!("Lendo arquivo: {}", input_path);
//     let sample_bytes = match fs::read(input_path) {
//         Ok(bytes) => bytes,
//         Err(e) => {
//             eprintln!("Erro ao ler o arquivo '{}': {}", input_path, e);
//             return;
//         }
//     };

//     let original_size = sample_bytes.len();
//     println!("Tamanho original: {} bytes\n", original_size);

//     println!("{:-<60}", "");
//     println!(
//         "{:<10} | {:<20} | {:<15}",
//         "Ordem (N)", "Tamanho Comprimido", "Razão (%)"
//     );
//     println!("{:-<60}", "");

//     for n in 1..=10 {
//         let mut ppmc_encoder = PPMC::new(n);
//         let compressed_cursor = Cursor::new(Vec::new());
//         let mut writer = BitWriter::new(compressed_cursor);
//         let mut encoder = ArithmeticEncoder::new(48);

//         // Executa a compressão
//         ppmc_encoder
//             .encode(&sample_bytes, &mut encoder, &mut writer)
//             .unwrap();

//         // Finaliza o bitstream
//         encoder
//             .encode(EOF_SYMBOL, &ppmc_encoder.minus_one, &mut writer)
//             .unwrap();
//         encoder.finish_encode(&mut writer).unwrap();
//         writer.pad_to_byte().unwrap();

//         // Calcula os resultados
//         let compressed_bytes = writer.get_ref().get_ref().clone();
//         let compressed_size = compressed_bytes.len();
//         let ratio = (compressed_size as f64 / original_size as f64) * 100.0;

//         println!("{:<10} | {:<20} | {:.2}%", n, compressed_size, ratio);

//         let cursor = Cursor::new(compressed_bytes.clone());
//         let mut reader = BitReader::<_, MSB>::new(cursor);
//         let mut decoder = ArithmeticDecoder::new(48);

//         let decompressed_bytes = ppmc_encoder.decode(&mut decoder, &mut reader).unwrap();
//     }

//     println!("{:-<60}", "");
// }

fn main() {
    let input_path = "corpus/dickens";

    println!("Lendo arquivo: {}", input_path);

    // Lê o arquivo inteiro para a memória como Vec<u8>
    let sample_bytes = match fs::read(input_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Erro ao ler o arquivo '{}': {}", input_path, e);
            return;
        }
    };

    println!("Tamanho original: {} bytes", sample_bytes.len());

    // Setup Encoder
    let mut ppmc_encoder = PPMC::new(5);
    let compressed_cursor = Cursor::new(Vec::new());
    let mut writer = BitWriter::new(compressed_cursor);
    let mut encoder = ArithmeticEncoder::new(48);

    println!("Comprimindo...");
    ppmc_encoder
        .encode(&sample_bytes, &mut encoder, &mut writer)
        .unwrap();

    const EOF_SYMBOL: u32 = 257;
    encoder
        .encode(EOF_SYMBOL, &ppmc_encoder.minus_one, &mut writer)
        .unwrap();
    encoder.finish_encode(&mut writer).unwrap();
    writer.pad_to_byte().unwrap();

    let compressed_bytes = writer.get_ref().get_ref().clone();

    // Escrevendo arquivo comprimido
    let mut f_comp = File::create("Compressed.dd").expect("Erro ao criar arquivo");
    match f_comp.write_all(&compressed_bytes) {
        Ok(_) => println!(
            "Comprimido com sucesso! Tamanho final: {} bytes",
            compressed_bytes.len()
        ),
        Err(_) => println!("Erro ao salvar arquivo comprimido"),
    };

    // Setup do decoder
    println!("Descomprimindo...");
    let mut ppmc_decoder = PPMC::new(5);
    let cursor = Cursor::new(compressed_bytes.clone());
    let mut reader = BitReader::<_, MSB>::new(cursor);
    let mut decoder = ArithmeticDecoder::new(48);

    let decompressed_bytes = ppmc_decoder.decode(&mut decoder, &mut reader).unwrap();

    // Escrevendo arquivo descomprimido
    let mut f_decomp = File::create("Decompressed.out").expect("Erro ao criar arquivo");
    match f_decomp.write_all(&decompressed_bytes) {
        Ok(_) => println!("Descomprimido com sucesso!"),
        Err(_) => println!("Erro ao salvar arquivo descomprimido"),
    }

    // Verificação de integridade
    if sample_bytes == decompressed_bytes {
        println!("Os bytes descomprimidos são idênticos aos originais!");
    } else {
        println!("Os bytes descomprimidos não batem com os originais!");
    }
}
