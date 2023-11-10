//! Rigol DP800 API using the telnet interface.
//!
//! See the [DP800 Series Programming Guide] for more information.
//!
//! [DP800 Series Programming Guide]: https://www.batronix.com/pdf/Rigol/ProgrammingGuide/DP800_ProgrammingGuide_EN.pdf

use std::{
    io::{self, BufRead, BufReader, BufWriter, Write},
    net::{TcpStream, ToSocketAddrs},
    str::FromStr,
};

fn parse_error() -> io::Error {
    io::Error::new(io::ErrorKind::Other, "Parse error")
}

fn parse<F>(s: &str) -> io::Result<F>
where
    F: FromStr,
{
    if let Ok(id) = s.parse::<F>() {
        Ok(id)
    } else {
        Err(parse_error())
    }
}

/// Power supply identification strings.
///
/// Returned by [`Dp800::measure`].
#[derive(Debug)]
pub struct Measurement {
    /// Voltage in volts.
    pub voltage: f32,
    /// Current in amps.
    pub current: f32,
    /// Power in watts.
    pub power: f32,
}

impl FromStr for Measurement {
    type Err = io::Error;

    #[allow(clippy::get_first)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split: Vec<&str> = s.split(',').collect();
        Ok(Self {
            voltage: parse(split.get(0).ok_or_else(parse_error)?)?,
            current: parse(split.get(1).ok_or_else(parse_error)?)?,
            power: parse(split.get(2).ok_or_else(parse_error)?)?,
        })
    }
}

/// Power supply identification strings.
///
/// Returned by [`Dp800::identify`].
#[derive(Debug, PartialEq, Eq)]
pub struct Identify {
    /// Manufacturer name
    pub manufacturer: String,
    /// Instrument model
    pub model: String,
    /// Instrument serial number
    pub sn: String,
    /// Digital board version number
    pub version: String,
}

impl FromStr for Identify {
    type Err = io::Error;

    #[allow(clippy::get_first)]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split: Vec<&str> = s.split(',').collect();
        Ok(Self {
            manufacturer: split.get(0).ok_or_else(parse_error)?.to_string(),
            model: split.get(1).ok_or_else(parse_error)?.to_string(),
            sn: split.get(2).ok_or_else(parse_error)?.to_string(),
            version: split.get(3).ok_or_else(parse_error)?.to_string(),
        })
    }
}

enum State {
    Off,
    On,
}

impl From<bool> for State {
    fn from(b: bool) -> Self {
        match b {
            true => Self::On,
            false => Self::Off,
        }
    }
}

impl From<State> for bool {
    fn from(s: State) -> Self {
        match s {
            State::Off => false,
            State::On => true,
        }
    }
}

impl FromStr for State {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ON" => Ok(State::On),
            "OFF" => Ok(State::Off),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::On => write!(f, "ON"),
            Self::Off => write!(f, "OFF"),
        }
    }
}

/// DP800 power supply.
///
/// # Channel Indexing
///
/// * Channels are 1-indexed
/// * Out-of-bounds values for channels will return the value for the
///   currently selected channel
pub struct Dp800 {
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
}

impl Dp800 {
    pub fn connect<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let stream: TcpStream = std::net::TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;
        Ok(Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: BufWriter::new(stream),
        })
    }

    fn cmd(&mut self, cmd: &str) -> io::Result<()> {
        self.writer.write_all(cmd.as_bytes())?;
        self.writer.flush()
    }

    fn q(&mut self, query: &str) -> io::Result<String> {
        let mut buf: String = String::with_capacity(64);
        {
            self.writer.write_all(query.as_bytes())?;
            self.writer.flush()?;
            self.reader.read_line(&mut buf)?;
        }
        buf.pop();
        Ok(buf)
    }

    fn q_parse<F>(&mut self, query: &str) -> io::Result<F>
    where
        F: FromStr,
    {
        let s: String = self.q(query)?;
        parse::<F>(s.as_str())
    }

    fn q_bool(&mut self, query: &str) -> io::Result<bool> {
        let state: State = self.q_parse(query)?;
        Ok(state.into())
    }

    /// Idenitfy the power supply.
    pub fn identify(&mut self) -> io::Result<Identify> {
        self.q_parse("*IDN?\n")
    }

    /// Output state.
    pub fn output_state(&mut self, ch: u8) -> io::Result<bool> {
        self.q_bool(format!(":OUTP? CH{ch}\n").as_str())
    }

    /// Set the output state.
    pub fn set_output_state(&mut self, ch: u8, state: bool) -> io::Result<()> {
        let state: State = state.into();
        self.cmd(format!(":OUTP CH{ch},{state}\n").as_str())
    }

    /// Currently selected channel.
    pub fn ch(&mut self) -> io::Result<u8> {
        self.q_parse(":INST:NSEL?\n")
    }

    /// Select a channel.
    pub fn set_ch(&mut self, ch: u8) -> io::Result<()> {
        self.cmd(format!(":INST:NSEL {ch}\n").as_str())
    }

    /// Setpoint current in Amps.
    pub fn current(&mut self, ch: u8) -> io::Result<f32> {
        self.q_parse(format!(":SOUR{ch}:CURR?\n").as_str())
    }

    /// Set the current setpoint in Amps.
    pub fn set_current(&mut self, ch: u8, amps: f32) -> io::Result<()> {
        self.cmd(format!(":SOUR{ch}:CURR {amps:.3}\n").as_str())
    }

    /// Setpoint voltage in Volts.
    pub fn voltage(&mut self, ch: u8) -> io::Result<f32> {
        self.q_parse(format!(":SOUR{ch}:VOLT?\n").as_str())
    }

    /// Set the voltage setpoint in Volts.
    pub fn set_voltage(&mut self, ch: u8, volts: f32) -> io::Result<()> {
        self.cmd(format!(":SOUR{ch}:VOLT {volts:.3}\n").as_str())
    }

    /// Get a measurement of voltage, current, and power.
    pub fn measure(&mut self, ch: u8) -> io::Result<Measurement> {
        self.q_parse(format!(":MEAS:ALL? CH{ch}\n").as_str())
    }

    /// Over current protection value in Amps.
    pub fn ocp(&mut self, ch: u8) -> io::Result<f32> {
        self.q_parse(format!(":OUTP:OCP:VAL? CH{ch}\n").as_str())
    }

    /// Set the over current protection value in Amps.
    pub fn set_ocp(&mut self, ch: u8, amps: f32) -> io::Result<()> {
        self.cmd(format!(":OUTP:OCP:VAL CH{ch},{amps:.3}\n").as_str())
    }

    /// Returns `true` if over current protection is enabled.
    pub fn ocp_on(&mut self, ch: u8) -> io::Result<bool> {
        self.q_bool(format!(":OUTP:OCP:STAT? CH{ch}\n").as_str())
    }

    /// Enable or disable over current protection.
    pub fn set_ocp_on(&mut self, ch: u8, on: bool) -> io::Result<()> {
        let state: State = on.into();
        self.cmd(format!(":OUTP:OCP:STAT CH{ch},{state}\n").as_str())
    }

    /// Over voltage protection value in Volts.
    pub fn ovp(&mut self, ch: u8) -> io::Result<f32> {
        self.q_parse(format!(":OUTP:OVP:VAL? CH{ch}\n").as_str())
    }

    /// Set the over voltage protection value in Volts.
    pub fn set_ovp(&mut self, ch: u8, volts: f32) -> io::Result<()> {
        self.cmd(format!(":OUTP:OVP:VAL CH{ch},{volts:.3}\n").as_str())
    }

    /// Returns `true` if over voltage protection is enabled.
    pub fn ovp_on(&mut self, ch: u8) -> io::Result<bool> {
        self.q_bool(format!(":OUTP:OVP:STAT? CH{ch}\n").as_str())
    }

    /// Enable or disable over voltage protection.
    pub fn set_ovp_on(&mut self, ch: u8, on: bool) -> io::Result<()> {
        let state: State = on.into();
        self.cmd(format!(":OUTP:OVP:STAT CH{ch},{state}\n").as_str())
    }
}
