use crate::{
    library::{
        client::TangleTunesClient,
        tcp::{RequestChunksEncoder, SendChunksDecoder},
        util::SongId,
    },
    BYTES_PER_CHUNK_USIZE,
};
use bytes::BytesMut;
use ethers::{types::Address, utils::keccak256};
use ethers_providers::StreamExt;
use futures::{SinkExt, Stream};
use num_integer::div_ceil;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio_util::codec::{FramedRead, FramedWrite};

const CHUNKS_PER_REQUEST: usize = 20;
const CONCURRENT_REQUESTS: usize = 3;

impl TangleTunesClient {
    /// Downloads the chunks from the smart-contract and verifies them against the given song-data.
    pub async fn verify_chunks_against_smart_contract(
        &self,
        song_id: SongId,
        song_data: &[u8],
        first_chunk_id: usize,
    ) -> eyre::Result<bool> {
        let chunks = div_ceil(song_data.len(), BYTES_PER_CHUNK_USIZE);
        let contract_hashes = self.call_check_chunks(song_id, first_chunk_id, chunks).await?;
        let calculated_hashes = song_data
            .chunks(BYTES_PER_CHUNK_USIZE)
            .map(keccak256)
            .map(Into::into)
            .collect::<Vec<SongId>>();
        assert_eq!(calculated_hashes.len(), chunks);

        if contract_hashes == calculated_hashes {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Download chunks from the distributor
    pub async fn download_from_distributor(
        &'static self,
        socket_address: SocketAddr,
        song_id: SongId,
        first_chunk_id: usize,
        chunk_amount: usize,
        distributor_address: Address,
    ) -> eyre::Result<Vec<u8>> {
        let last_chunk_id = first_chunk_id + chunk_amount;

        let mut stream = TcpStream::connect(socket_address).await?;
        let (read_stream, write_stream) = stream.split();
        let mut read_stream = FramedRead::new(read_stream, SendChunksDecoder::new());
        let mut write_stream = FramedWrite::new(write_stream, RequestChunksEncoder);

        let mut request_queue = RequestQueue::new(first_chunk_id, last_chunk_id);
        let mut song = Vec::with_capacity(chunk_amount);

        // While our song has not yet been completely downloaded..
        while !song_is_complete(&song, chunk_amount) {
            // .. send requests if necessary
            while let Some((request_id, request_size)) = request_queue.request_now(&song) {
                println!("Requesting {request_size} chunks starting at id {request_id}");

                let tx_rlp = self
                    .create_get_chunks_signed_rlp(
                        song_id.clone(),
                        request_id,
                        request_size,
                        distributor_address,
                    )
                    .await?;
                write_stream.send(&tx_rlp.0).await?;
                break;
            }

            // And then read the next response
            add_next_to_buffer(&mut read_stream, &mut song).await?
        }

        if self
            .verify_chunks_against_smart_contract(song_id, &song, first_chunk_id)
            .await?
        {
            println!("Verification of song hashes successful!");
            Ok(song)
        } else {
            Err(eyre!("Song verification failed"))
        }
    }
}

/// Whether the song is completely downloaded, given the amount of chunks that it should contain.
fn song_is_complete(song: &[u8], chunks: usize) -> bool {
    song.len() + BYTES_PER_CHUNK_USIZE > (chunks * BYTES_PER_CHUNK_USIZE)
}

/// Reads the next chunk from the stream and adds them to the buffer.
async fn add_next_to_buffer(
    read_stream: &mut (impl Stream<Item = eyre::Result<(u32, BytesMut)>> + Unpin),
    buffer: &mut Vec<u8>,
) -> eyre::Result<()> {
    let result = read_stream.next().await.ok_or(eyre!(
        "Distributor closed stream before all data was received"
    ))?;
    let (start_chunk_id, chunks) = result?;
    println!(
        "Received {} bytes starting at id {start_chunk_id}",
        chunks.len()
    );
    for (chunk, chunk_id) in chunks.chunks(BYTES_PER_CHUNK_USIZE).zip(start_chunk_id..) {
        assert_eq!(
            chunk_id as usize,
            (buffer.len() + start_chunk_id as usize) / BYTES_PER_CHUNK_USIZE
        );
        buffer.extend(chunk);
    }
    Ok(())
}

//------------------------------------------------------------------------------------------------
//  RequestQueue
//------------------------------------------------------------------------------------------------

/// A queue of requests for a (part of a) song.
struct RequestQueue(Vec<(usize, usize)>);

impl RequestQueue {
    /// Create a new request-queue that requests chunks from start-end
    pub fn new(first_chunk_id: usize, last_chunk_id: usize) -> Self {
        let inner = (first_chunk_id..last_chunk_id)
            .filter(|chunk_id| chunk_id % CHUNKS_PER_REQUEST == 0 || *chunk_id == last_chunk_id)
            .map(|chunk_id| {
                (
                    chunk_id,
                    Ord::min(CHUNKS_PER_REQUEST, last_chunk_id - chunk_id),
                )
            })
            .rev()
            .collect::<Vec<_>>();
        Self(inner)
    }

    /// Whether a new request should be made now.
    ///
    /// Returns (chunk_id, amount_of_chunks).
    pub fn request_now(&mut self, song: &[u8]) -> Option<(usize, usize)> {
        if let Some((request_id, _)) = self.0.last() {
            if *request_id <= (song.len() * BYTES_PER_CHUNK_USIZE) + CONCURRENT_REQUESTS {
                return Some(self.0.pop().unwrap());
            }
        };
        None
    }
}

#[cfg(test)]
mod test {
    use crate::{
        library::{
            app::AppData,
            client::download::{RequestQueue, CHUNKS_PER_REQUEST},
        },
        test::VALIDATED_SONG_HEX_ID,
        BYTES_PER_CHUNK_USIZE,
    };

    use super::song_is_complete;

    #[ignore]
    #[tokio::test]
    async fn test() -> eyre::Result<()> {
        let app = AppData::init_for_test(None, false).await?;
        let song_id = VALIDATED_SONG_HEX_ID.parse()?;
        let chunks = app.database.get_chunks(&song_id, 0, 20).await?;
        assert!(
            app.client
                .verify_chunks_against_smart_contract(song_id, &chunks, 0)
                .await?
        );
        Ok(())
    }

    #[test]
    fn song_is_complete_test() {
        assert!(song_is_complete(&[0; 1], 1));
        assert!(song_is_complete(&[0; BYTES_PER_CHUNK_USIZE], 1));
        assert!(!song_is_complete(&[0; BYTES_PER_CHUNK_USIZE], 2));
        assert!(song_is_complete(&[0; BYTES_PER_CHUNK_USIZE + 1], 2));
        assert!(song_is_complete(&[0; BYTES_PER_CHUNK_USIZE * 2 - 1], 2));
        assert!(song_is_complete(&[0; BYTES_PER_CHUNK_USIZE * 2], 2));
    }

    #[test]
    fn request_queue_test() {
        let requests = CHUNKS_PER_REQUEST * 4 - 1;
        let song = vec![0; BYTES_PER_CHUNK_USIZE * requests];
        let mut queue = RequestQueue::new(0, requests);

        assert_eq!(
            queue.request_now(&song),
            Some((0 * CHUNKS_PER_REQUEST, CHUNKS_PER_REQUEST))
        );
        assert_eq!(
            queue.request_now(&song),
            Some((1 * CHUNKS_PER_REQUEST, CHUNKS_PER_REQUEST))
        );
        assert_eq!(
            queue.request_now(&song),
            Some((2 * CHUNKS_PER_REQUEST, CHUNKS_PER_REQUEST))
        );
        assert_eq!(
            queue.request_now(&song),
            Some((3 * CHUNKS_PER_REQUEST, CHUNKS_PER_REQUEST - 1))
        );
    }
}
