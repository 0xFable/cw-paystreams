#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Uint128, CosmosMsg, BankMsg, Coin, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Timestamp};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{LookupStreamResponse, CountResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{State, STATE, PaymentStream, STREAMS};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw-1620";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        count: msg.count,
        owner: info.sender.clone(),
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender)
        .add_attribute("count", msg.count.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::CreateStream { recipient, deposit, token_addr, start_time, stop_time } => try_create_stream(deps, info, recipient, deposit, token_addr, start_time, stop_time),
        ExecuteMsg::WithdrawFromStream { recipient, amount, denom } => try_withdraw_from_stream(deps, info, env, recipient, amount, denom),
    }
}

pub fn try_create_stream(deps: DepsMut, info: MessageInfo, recipient: String, deposit: Uint128, token_addr: String, start_time: u64, stop_time: u64) -> Result<Response, ContractError> {
    
    let recipient = deps.api.addr_validate(&recipient)?;
    let duration = stop_time.checked_sub(start_time).unwrap_or_else(| | return 0);
    let deposit_amount: Uint128 = info
    .funds
    .iter()
    .find(|c| c.denom == String::from("uusd"))
    .map(|c| Uint128::from(c.amount))
    .unwrap_or_else(Uint128::zero);
    println!("Deposit amount {:?} Other amount {:?}", deposit, deposit_amount);

    if deposit_amount < deposit {
        return Err(ContractError::NotEnoughAvailableFunds);
    }

    let rate_per_second = deposit.checked_div(Uint128::from(duration)).unwrap_or_else(|_| return Uint128::zero());
    let stream_data = PaymentStream {
        recipient: recipient.clone(),
        deposit: deposit_amount,
        token_addr: deps.api.addr_validate(&token_addr)?,
        start_time: start_time,
        stop_time: stop_time,
        is_entity: false,
        rate_per_second: rate_per_second,
        remaining_balance: Uint128::from(deposit_amount),
        sender: info.sender.clone()
    };
    STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
        state.count += 1;
        Ok(state)
    })?;
    STREAMS.save(deps.storage, (&recipient, &info.sender), &stream_data)?;
    Ok(Response::new().add_attribute("method", "try_create_stream"))
}


pub fn try_withdraw_from_stream(deps: DepsMut, info: MessageInfo, env: Env, recipient: String, amount: Uint128, denom: String) -> Result<Response, ContractError> {
    let recipient = deps.api.addr_validate(&recipient)?;

    // Check amount is valid
    if amount == Uint128::zero(){
        return Err(ContractError::InvalidAmount{});
    }
    // TODO: Review if this access is right (&info.sender, &recipient)
    let mut state = STREAMS.load(deps.storage, (&recipient, &info.sender))?;

    // Check it doesn't exceed available
    let balance: Uint128 = balance_of(state.clone(), env).unwrap_or_else(|_| return Uint128::zero());
    println!("Amount {:?} Balance: {:?} Remaining from stream {:?}", amount, balance, state.remaining_balance);

    if amount > balance {
        return Err(ContractError::NotEnoughAvailableBalance{});
    }
    if amount > state.remaining_balance {
        return Err(ContractError::NotEnoughAvailableFunds{});
    }


    // Make the payout happen
    let payout_msg: CosmosMsg<> = CosmosMsg::Bank(BankMsg::Send {
        to_address: recipient.to_string(),
        amount: vec![Coin {
            denom: denom,
            amount: amount,
        }],
    });
    
    state.remaining_balance = state.remaining_balance.checked_sub(amount)?;
    STREAMS.save(deps.storage, (&recipient, &info.sender), &state)?;

    Ok(Response::new().add_attribute("method", "try_withdraw_from_stream").add_message(payout_msg))
}

pub fn balance_of(stream:PaymentStream, env: Env) -> StdResult<Uint128>{
    // Get the time delta 
    let delta = delta(stream.clone(), env)?;
    // Use delta to get the balance that should be available 
    let rec_bal = Uint128::from(delta).checked_mul(stream.rate_per_second)?;
    // println!("Delta: {:?}, Recipient Balance: {:?}", delta, rec_bal);

    if stream.deposit >= stream.remaining_balance {
        // println!("Deposit: {:?}, Remaining: {:?}", stream.deposit , stream.remaining_balance);
        let amount_available = stream.deposit.checked_sub(stream.remaining_balance)?;
        let new_balance = rec_bal.checked_sub(amount_available)?;
        // println!("New balance {:?}", new_balance);
        return Ok(new_balance);
    }
    return Ok(Uint128::from(0u128));
}

pub fn delta(stream: PaymentStream, env:Env) -> StdResult<u64>{
    if env.block.time <= Timestamp::from_seconds(stream.start_time) {
        return Ok(0);
    }
    if env.block.time < Timestamp::from_seconds(stream.stop_time) {
        return Ok(env.block.time.minus_seconds(stream.start_time).seconds());
    }
    let duration = stream.stop_time.checked_sub(stream.start_time).unwrap_or_else(| | return 0);
    return Ok(Timestamp::from_seconds(duration).seconds());
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::LookupStream { payer, payee } => to_binary(&query_stream(deps, payer, payee)?),
        QueryMsg::StreamCount { } => to_binary(&query_stream_count(deps)?),
    }
}

fn query_stream_count(deps: Deps) -> StdResult<CountResponse> {
    let state = STATE.load(deps.storage)?;
    Ok(CountResponse { count: state.count })
}

fn query_stream(deps: Deps, payer: String, payee: String) -> StdResult<LookupStreamResponse> {
    // For now, just validate what we got was a valid address
    let _vld_payer = deps.api.addr_validate(&payer)?;
    let _vld_payee = deps.api.addr_validate(&payee)?;
    let state = STREAMS.load(deps.storage, (&_vld_payer, &_vld_payee))?;
    Ok(LookupStreamResponse { stream: state })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary};
    use cosmwasm_std::{Uint128, CosmosMsg, BankMsg, Coin, SubMsg, Timestamp};


    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg { count: 0 };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::StreamCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(0, value.count);
    }

    #[test]
    fn can_create_stream() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg { count: 0 };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::StreamCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(0, value.count);

        // Create a Stream between mock users
        let payer = mock_info("payer", &coins(1000, "uusd"));

        let payee = mock_info("payee", &[]);

        let env = mock_env();

        let stream_msg = ExecuteMsg::CreateStream{
            deposit: Uint128::new(100),
            recipient: payee.sender.to_string(),
            start_time: env.block.time.seconds(),
            stop_time: env.block.time.seconds() + 100,
            token_addr: String::from("uusd")
        };

        let _ = execute(deps.as_mut(), env.clone(), payer.clone(), stream_msg).unwrap();

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::StreamCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(1, value.count);

    }

    #[test]

    // Idea of this test is too ensure that when a payer creates a stream that their specified deposit amount is not more than the funds they provide to fund the stream
    fn can_not_create_with_deposit_more_than_provided_funds() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg { count: 0 };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::StreamCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(0, value.count);

        // Create 'payer' user with 1000 USD in balance to act as the payer of the stream
        let payer = mock_info("payer", &coins(1000, "uusd"));

        // Create 'payee' user with 0 USD in balance to act as the receiver of funds from the stream
        let payee = mock_info("payee", &coins(0, "uusd"));

        let env = mock_env();

        // Attempt to create a stream with 1 more dollar than provided. Cheating the system out of a dollar. 
        // Basically "Hey lil dude from across the street, lemme hold a dollar"
        let stream_msg = ExecuteMsg::CreateStream{
            deposit: Uint128::new(1001),
            recipient: payee.sender.to_string(),
            start_time: env.block.time.seconds(),
            stop_time: env.block.time.seconds() + 100,
            token_addr: String::from("uusd")
        };

        // We need this to unwrap as an error
        let _ = execute(deps.as_mut(), env.clone(), payer.clone(), stream_msg).unwrap_err();

        // Attempt to create a stream with the correct deposit and provided funds
        let stream_msg = ExecuteMsg::CreateStream{
            deposit: Uint128::new(1000),
            recipient: payee.sender.to_string(),
            start_time: env.block.time.seconds(),
            stop_time: env.block.time.seconds() + 100,
            token_addr: String::from("uusd")
        };
        // No issue
        let _ = execute(deps.as_mut(), env.clone(), payer.clone(), stream_msg).unwrap();

        // TODO: Probably a test on its own, what if user provides too much monies but their specified deposit is not enough? 
        // Maybe reject? Or have a refund option?

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::StreamCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(1, value.count);

    }

    #[test]
    fn can_withdraw_from_a_created_stream_happy_and_sad_path() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg { count: 0 };
        // Creator user will create the stream
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::StreamCount {}).unwrap();
        let value: CountResponse = from_binary(&res).unwrap();
        assert_eq!(0, value.count);

        // Create 'payer' user with 1000 USD in balance to act as the payer of the stream
        let payer = mock_info("payer", &coins(100, "uusd"));

        // Create 'payee' user with 0 USD in balance to act as the receiver of funds from the stream
        let payee = mock_info("payee", &coins(0, "uusd"));

        let mut env = mock_env();

        let stream_msg = ExecuteMsg::CreateStream{
            deposit: Uint128::new(100),
            recipient: payee.sender.to_string(),
            start_time: env.block.time.seconds(),
            stop_time: env.block.time.seconds() + 100,
            token_addr: String::from("uusd")
        };

        let execute_res = execute(deps.as_mut(), env.clone(), payer.clone(), stream_msg).unwrap();

        assert_eq!(execute_res.events, []);

         // Verify the payee cant get all right away
         let withdraw_msg = ExecuteMsg::WithdrawFromStream{
            amount: Uint128::new(90),
            denom: String::from("uusd"),
            recipient: payee.sender.to_string()
        };
        let execute_res = execute(deps.as_mut(), env.clone(), payer.clone(), withdraw_msg).unwrap_err();

        match execute_res {
            ContractError::NotEnoughAvailableBalance {} => {}
            _ => panic!("DO NOT ENTER HERE"),
        }

        env.block.time = Timestamp::from_seconds(env.block.time.seconds()+10);
        let withdraw_msg = ExecuteMsg::WithdrawFromStream{
            amount: Uint128::new(10),
            denom: String::from("uusd"),
            recipient: payee.sender.to_string()
        };
        let execute_res = execute(deps.as_mut(), env.clone(), payer.clone(), withdraw_msg).unwrap();
        assert_eq!(1, execute_res.messages.len());

        // Verify the payee has indeed been paid by verifying the attached bank submessage
        assert_eq!(execute_res.messages, vec![SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
            to_address: "payee".to_string(),
            amount: vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(10u128),
            }],
        }))]);

        // Verify the payee cant get all right away
        let withdraw_msg = ExecuteMsg::WithdrawFromStream{
            amount: Uint128::new(90),
            denom: String::from("uusd"),
            recipient: payee.sender.to_string()
        };
        let execute_res = execute(deps.as_mut(), env.clone(), payer.clone(), withdraw_msg).unwrap_err();

        match execute_res {
            ContractError::NotEnoughAvailableBalance {} => {}
            _ => panic!("DO NOT ENTER HERE"),
        }

        // Simulate the rest of the time, payee is able to get more now
        // env.block.height += 70;
        env.block.time = Timestamp::from_seconds(env.block.time.seconds()+90);
        let withdraw_msg = ExecuteMsg::WithdrawFromStream{
            amount: Uint128::new(90),
            denom: String::from("uusd"),
            recipient: payee.sender.to_string()
        };
        let execute_res = execute(deps.as_mut(), env.clone(), payer.clone(), withdraw_msg).unwrap();

        assert_eq!(1, execute_res.messages.len());

        // Verify the payee has indeed been paid by verifying the attached bank submessage
        assert_eq!(execute_res.messages, vec![SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
            to_address: "payee".to_string(),
            amount: vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(90u128),
            }],
        }))]);

        // Now this time, having drained the funds. No more should be sent right ? 
        // env.block.height += 51;
        env.block.time = Timestamp::from_seconds(env.block.time.seconds()+51);
        let withdraw_msg = ExecuteMsg::WithdrawFromStream{
            amount: Uint128::new(10),
            denom: String::from("uusd"),
            recipient: payee.sender.to_string()
        };
        let execute_res = execute(deps.as_mut(), env.clone(), payer, withdraw_msg).unwrap_err();

        match execute_res {
            ContractError::NotEnoughAvailableBalance {} => {}
            _ => panic!("DO NOT ENTER HERE"),
        }
    }
}
