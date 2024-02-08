
use candid::{CandidType, Decode, Deserialize, Encode, Principal};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{DefaultMemoryImpl,BoundedStorable, StableBTreeMap, Storable};
use std::borrow::BorrowMut;
use std::hash::Hash;
use std::{borrow::Cow, cell::RefCell}; 
use std::collections::HashMap;

#[derive(CandidType,Deserialize,Eq,PartialEq,Hash,Clone)]
enum Status {
    Rejected,
    Applied,
    Round(u8),
    Accepted,
}
#[derive(Clone,CandidType,Deserialize)]
enum Account {
    Applicant(Applicant),
    Employer(Employer),
}


#[derive(Clone,CandidType,Deserialize)]
struct Employer{
    pub name: String,
    pub organisation: String,
}

#[derive(Clone,CandidType,Deserialize)]
struct Applicant{
    pub applicant_id: Principal,
    name: String,
    bio: String,
    applied_jobs: Vec<usize>,
}


type Memory = VirtualMemory<DefaultMemoryImpl>;


#[derive(CandidType,Deserialize,Eq,PartialEq,Hash,Clone)]
struct Job {
    pub id: usize,
    pub num_rounds: u8,
    pub name: String,
    pub description: String,
    pub owner: Principal,
}
#[derive(CandidType, Deserialize,Clone)]
struct DappHireService {
    job_status_list: HashMap<usize,HashMap<Status, Vec<Principal>>>,
    jobs: HashMap<usize,Job>,
    profiles: HashMap<Principal,Account>,
}


impl Storable for DappHireService {
    fn to_bytes(&self) -> Cow<[u8]> {
        Cow::Owned(Encode!(self).unwrap())
    }

    fn from_bytes(bytes: Cow<[u8]>) -> Self {
        Decode!(bytes.as_ref(), Self).unwrap()
    }
}

const MAX_VALUE_SIZE: u32 = 10000;

// Implement BoundedStorable for Event
impl BoundedStorable for DappHireService
{
    const MAX_SIZE: u32 = MAX_VALUE_SIZE; // Adjust the size as needed
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
    RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
 
    static HIRE_SERVICE_MAP: RefCell<StableBTreeMap<u64, DappHireService , Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))),
        )
    );
}

const SERVICE_ID: u64 = 0;
// Function to create new Profile
#[ic_cdk::update]
fn create_applicant_account(name: String, bio: String )->Result<(),String> {
    HIRE_SERVICE_MAP.with(|hire_ref | {
        let mut service = hire_ref.borrow_mut().get(&SERVICE_ID).unwrap();
        let profile = service.borrow_mut().profiles.borrow_mut().get(&ic_cdk::api::caller());
        return match profile {
            Some(_account) => Err("Profile Already Exists".to_string()),
            None => {
                let new_profile = Account::Applicant(Applicant {
                    applicant_id: ic_cdk::api::caller(),
                    name,
                    bio,
                    applied_jobs: Vec::new(),
                });
                service.profiles.insert(ic_cdk::api::caller(), new_profile);
    
                Ok(())
            }
        }
    })
}

#[ic_cdk::update]
fn create_employer_account(name: String, organisation: String )->Result<(),String> {
    HIRE_SERVICE_MAP.with(|hire_ref | {
        let mut service = hire_ref.borrow_mut().get(&SERVICE_ID).unwrap();
        let profile = service.borrow_mut().profiles.borrow_mut().get(&ic_cdk::api::caller());
        return match profile {
            Some(_account) => Err("Profile Already Exists".to_string()),
            None => {
                let new_profile = Account::Employer(Employer {
                    name,
                    organisation
                });
                service.profiles.insert(ic_cdk::api::caller(), new_profile);
    
                Ok(())
            }
        }
    })
}

#[ic_cdk::query]
fn get_all_jobs() -> Vec<Job> {
    HIRE_SERVICE_MAP.with(|hire_ref | {
        let service = hire_ref.borrow_mut().get(&SERVICE_ID).unwrap();
        return service.jobs.into_iter().map(|(_,v)|  v ).collect();
    })
}


#[ic_cdk::query]
fn get_job_status(job_id: usize) -> HashMap<Status,Vec<Principal>> {
    HIRE_SERVICE_MAP.with(|hire_ref | {
        let service = hire_ref.borrow_mut().get(&SERVICE_ID).unwrap();
        return service.job_status_list.get(&job_id).cloned().unwrap();
    })
}

#[ic_cdk::query]
fn get_profile(profile_id: Principal) -> Account {
    HIRE_SERVICE_MAP.with(|hire_ref | {
        let service = hire_ref.borrow_mut().get(&SERVICE_ID).unwrap();
        return service.profiles.get(&profile_id).cloned().unwrap();
    })
}

//create new job
#[ic_cdk::update]
async fn create_job(name: String,description: String,num_rounds: u8) -> Job {
    HIRE_SERVICE_MAP.with(|hire_ref | {
        let mut service = hire_ref.borrow_mut().get(&SERVICE_ID).unwrap();
        let len = service.jobs.len();
        let job = Job {
            id: len,
            num_rounds: num_rounds,
            name: name,
            description: description,
            owner: ic_cdk::api::caller(),
        };
        service.jobs.insert(len, job);
        return service.jobs.get(&len).cloned().unwrap();
    })
}

#[ic_cdk::update]
fn move_application_status(job_id: usize, applicant_id: Principal) -> Result<(), String> {
    HIRE_SERVICE_MAP.with(|hire_ref | {
        let mut service = hire_ref.borrow_mut().get(&SERVICE_ID).unwrap();
        let job = service.jobs.get(&job_id).ok_or("Job not found")?;
        assert_eq!(job.owner, ic_cdk::api::caller());

        if let Some(account) = service.profiles.get(&ic_cdk::api::caller()) {
            if let Account::Employer(_) = account {
                let job_status = service.job_status_list.entry(job_id)
                    .or_insert_with(HashMap::new);
                let jobs = &mut job_status.clone();
                let status_list = job_status.entry(Status::Applied).or_default();
                let rejected_list = jobs.entry(Status::Rejected).or_default();
                if status_list.contains(&applicant_id) {
                    // Move from Applied to Round(1)
                    status_list.retain(|x| *x != applicant_id);
                    job_status.entry(Status::Round(1)).or_default().push(applicant_id);
                } 
                else if rejected_list.contains(&applicant_id) {
                    return Err("Application already rejected!".to_string());
                }else {
                    // Check if the applicant is already in a round
                    for round in 1..=job.num_rounds {
                        let status_list = job_status.entry(Status::Round(round)).or_default();
                        if status_list.contains(&applicant_id) {
                            // Move to the next round or accept
                            if round == job.num_rounds {
                                // Accept the applicant
                                status_list.retain(|x| *x != applicant_id);
                                job_status.entry(Status::Accepted).or_default().push(applicant_id);
                            } else {
                                // Move to the next round
                                status_list.retain(|x| *x != applicant_id);
                                job_status.entry(Status::Round(round + 1)).or_default().push(applicant_id);
                            }
                            break;
                        }
                    }
                }
                Ok(())
            } else {
                Err("Applicants are not allowed to do this".to_string())
            }
        } else {
            Err("You have to create an employer account to move application".to_string())
        }
    })
}


#[ic_cdk::update]
fn reject_application_status(job_id: usize, applicant_id: Principal) -> Result<(), String> {
    HIRE_SERVICE_MAP.with(|hire_ref | {
        let mut service = hire_ref.borrow_mut().get(&SERVICE_ID).unwrap();
        let job = service.jobs.get(&job_id).ok_or("Job not found")?;
        assert_eq!(job.owner, ic_cdk::api::caller());

        if let Some(account) = service.profiles.get(&ic_cdk::api::caller()) {
            if let Account::Employer(_) = account {
                let job_status = service.job_status_list.entry(job_id)
                    .or_insert_with(HashMap::new);
                let jobs = &mut job_status.clone();
                let status_list = jobs.entry(Status::Applied).or_default();
                let rejected_list = job_status.entry(Status::Rejected).or_default();
                if status_list.contains(&applicant_id) {
                    // Move from Applied to Round(1)
                    status_list.retain(|x| *x != applicant_id);
                    job_status.entry(Status::Rejected).or_default().push(applicant_id);
                } 
                else if rejected_list.contains(&applicant_id) {
                    return Err("Application already rejected!".to_string());
                }
                else {
                    // Check if the applicant is already in a round
                    for round in 1..=job.num_rounds {
                        let status_list = job_status.entry(Status::Round(round)).or_default();
                        status_list.retain(|x| *x != applicant_id);
                        job_status.entry(Status::Rejected).or_default().push(applicant_id);
                    }
                }
                Ok(())
            } else {
                Err("Applicants are not allowed to do this".to_string())
            }
        } else {
            Err("You have to create an employer account to move application".to_string())
        }
    })
}

#[ic_cdk::query]
fn view_private_applications(applicant_id: Principal) -> Result<Applicant, String> {
    HIRE_SERVICE_MAP.with(|hire_ref | {
        let service = hire_ref.borrow().get(&SERVICE_ID).unwrap();
        let current_account = service.profiles.get(&ic_cdk::api::caller()).cloned().unwrap();

        if let Account::Employer(_) = current_account {
            let applicant_account = service.profiles.get(&applicant_id)
                .ok_or("Applicant account not found")?;

            match applicant_account {
                Account::Applicant(applicant) => {
                    let mut owns_job = false;
                    for job_id in &applicant.applied_jobs {
                        if let Some(job) = service.jobs.get(job_id) {
                            if job.owner == ic_cdk::api::caller() {
                                owns_job = true;
                                break;
                            }
                        }
                    }

                    if owns_job {
                        Ok(applicant.clone())
                    } else {
                        Err("Caller does not own any of the applied jobs".to_string())
                    }
                },
                _ => Err("The specified ID does not belong to an applicant".to_string()),
            }
        } else {
            Err("Caller is not an employer".to_string())
        }
    })
}
