use std::path::Path;
use crate::{CRDTSpan, KVPair, LocalVersion, NewOpLog, Time};
use crate::encoding::agent_assignment::{AAWriteCursor, AgentMapping};
use crate::encoding::PackWriter;
use crate::storage::wal::{WALError, WriteAheadLog};

struct WALChunks {
    wal: WriteAheadLog,

    // The WAL just stores changes in order. We don't need to worry about complex time DAG
    // traversal.
    next_version: Time,
}

impl WALChunks {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, WALError> {
        Ok(Self {
            wal: WriteAheadLog::open(path, |chunk| {
                dbg!(chunk);
                Ok(())
            })?,
            next_version: 0
        })
    }

    // fn parse_chunk(chunk: &[u8]) -> Result<(), WALError> {
    //     dbg!(chunk);
    //     Ok(())
    // }

    pub fn flush(&mut self, oplog: &NewOpLog) -> Result<(), WALError> {
        let next = oplog.len();

        if next == self.next_version {
            // Nothing to do!
            return Ok(());
        }

        // Data to store:
        //
        // - Agent assignment
        // - Parents

        self.wal.write_chunk(|buf| {
            let mut map = AgentMapping::new(&oplog.client_data);
            let mut aa_writer =
                PackWriter::new(AAWriteCursor::new(oplog.client_data.len()));

            for KVPair(_, span) in oplog.client_with_localtime.iter_range_packed((self.next_version..next).into()) {
                dbg!(span);

                let mapped_agent = map.map(&oplog.client_data, span.agent);
                aa_writer.push(CRDTSpan {
                    agent: mapped_agent,
                    seq_range: span.seq_range
                }, buf);
            }

            buf.extend_from_slice(&map.into_output());
            aa_writer.flush(buf);

            Ok(())
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::new_oplog::Primitive::I64;
    use crate::new_oplog::ROOT_MAP;
    use crate::NewOpLog;
    use crate::path::PathComponent;
    use crate::path::PathComponent::Key;
    use crate::storage::wal::WALError;
    use crate::storage::wal_encoding::WALChunks;

    #[test]
    fn simple_encode_test() {
        let mut oplog = NewOpLog::new();
        // dbg!(&oplog);

        let seph = oplog.get_or_create_agent_id("seph");
        let mike = oplog.get_or_create_agent_id("mike");
        let mut v = 0;

        oplog.set_at_path(seph, &[Key("name")], I64(1));
        oplog.set_at_path(seph, &[Key("name")], I64(2));
        oplog.set_at_path(seph, &[Key("name")], I64(3));
        oplog.set_at_path(mike, &[Key("name")], I64(3));
        // dbg!(oplog.checkout(&oplog.version));

        // dbg!(&oplog);
        oplog.dbg_check(true);

        let mut wal = WALChunks::open("test.wal").unwrap();
        wal.flush(&oplog).unwrap();
    }
}